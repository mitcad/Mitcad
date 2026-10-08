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
    // The polygon constraint (of two lines) is left out, so the sketch is
    // partial.
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
            .contains("PolygonConstraint: a polygon needs its corners and centre")
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
    /// Whether faces are listed with a point each (see face_point), so
    /// that face fingerprints resolve.
    face_listing: bool,
    /// Whether the solids this kernel's extrusions make come out 3e-4
    /// larger than the same extrusion of another kernel (a marker in their
    /// history), as an extrusion of regions next to the file's would.
    skew: bool,
    /// Whether solids without copies of a pattern all measure the same, so
    /// that only patterns tell extrusions of different regions apart.
    alike: bool,
    /// Runs in each kernel call of this name ("extrude_feature",
    /// "offset_faces", "mass_properties") before it is made: a call that
    /// blocks or takes long (mitcad#68, mitcad#69, mitcad#71, mitcad#82).
    #[allow(clippy::type_complexity)]
    on_call: std::cell::RefCell<Option<(&'static str, Box<dyn FnMut(&MockKernel) + Send>)>>,
    /// How many of the next extrusions fail as an operation whose
    /// allocation failed does (mitcad#80).
    out_of_memory: std::cell::Cell<usize>,
    /// Extrusions that take long, every time (mitcad#78).
    slow: Option<SlowExtrusions>,
    /// How many of them were asked for.
    slow_calls: std::cell::Cell<usize>,
    /// Whether extrusions against the sketch's normal differ from those
    /// along it (a marker in their history; the mock kernel's are alike).
    directed: bool,
}

/// Extrusions of `regions` (along the sketch's normal only, with `along`)
/// take `seconds`, checking for a request to stop as `slow_call`.
#[derive(Clone, Copy)]
struct SlowExtrusions {
    regions: &'static [&'static str],
    along: bool,
    seconds: f64,
}

impl TestKernel {
    fn hook(&self, call: &str) {
        if let Some((name, hook)) = self.on_call.borrow_mut().as_mut()
            && *name == call
        {
            hook(&self.mock);
        }
    }
}

/// A kernel call that takes up to 30 s, checking for a request to stop as
/// the geometry kernel's long operations do (`interruptible`).
fn slow_call(mock: &MockKernel) {
    slow_call_for(mock, 30.0);
}

/// A kernel call that takes up to `seconds`, as [`slow_call`].
fn slow_call_for(mock: &MockKernel, seconds: f64) {
    mock.stop_on_cancel.set(true);
    let begun = std::time::Instant::now();
    while begun.elapsed().as_secs_f64() < seconds {
        if mock
            .interrupt
            .borrow()
            .as_ref()
            .is_some_and(|m| m.is_cancelled())
        {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
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
        self.hook("extrude_feature");
        if self.out_of_memory.get() > 0 {
            self.out_of_memory.set(self.out_of_memory.get() - 1);
            return Err(KernelError::failed(format!(
                "extrude: {}",
                KernelError::OUT_OF_MEMORY
            )));
        }
        let keys: BTreeSet<String> = spec.regions.iter().map(|r| r.key.to_string()).collect();
        if let Some(slow) = self.slow
            && keys == slow.regions.iter().map(|r| r.to_string()).collect()
            && (!slow.along || spec.direction[2] > 0.0)
        {
            self.slow_calls.set(self.slow_calls.get() + 1);
            slow_call_for(&self.mock, slow.seconds);
        }
        let mut shape = self.mock.extrude_feature(spec)?;
        if self.skew {
            shape.history.push('~');
        }
        if self.directed && spec.direction[2] < 0.0 {
            shape.history.push_str("(flipped)");
        }
        Ok(shape)
    }

    fn solids(&self, shape: &MockShape) -> Result<Vec<MockShape>, KernelError> {
        self.mock.solids(shape)
    }

    fn interruptible<R>(&self, monitor: &Arc<RecomputeMonitor>, f: impl FnOnce() -> R) -> R {
        self.mock.interruptible(monitor, f)
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
        self.hook("mass_properties");
        let f = self.fingerprint(shape);
        if self.alike && !f.contains("moved(") {
            // (Less than a prism of either rectangle adds.)
            return Ok(MassProperties {
                volume: 1000.0,
                area: 1000.0,
                center: [0.0; 3],
            });
        }
        let scale = if f.contains('~') { 1.0003 } else { 1.0 };
        Ok(MassProperties {
            volume: hash(&f.replace('~', ""), 1) * scale,
            area: hash(&f.replace('~', ""), 2),
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

    // With face_listing, the faces (the mock's cylinders as such) and
    // each face's point (see face_point) as the nearest to any.
    fn faces(&self, shape: &MockShape) -> Result<Vec<mitcad_model::FaceInfo>, KernelError> {
        if !self.face_listing {
            return Err(KernelError::Unsupported("face listing"));
        }
        Ok(shape
            .faces
            .iter()
            .map(|f| mitcad_model::FaceInfo {
                names: vec![f.to_string()],
                surface: if self.mock.face_cylinder(shape, f).is_ok() {
                    "cylinder"
                } else {
                    "plane"
                }
                .to_owned(),
                area: 1.0,
            })
            .collect())
    }

    fn hole_tool(
        &self,
        spec: &mitcad_model::HoleSpec<'_, MockShape>,
    ) -> Result<MockShape, KernelError> {
        self.mock.hole_tool(spec)
    }

    fn modeled_thread(
        &self,
        body: &MockShape,
        spec: &mitcad_model::ThreadSpec<'_>,
    ) -> Result<MockShape, KernelError> {
        self.mock.modeled_thread(body, spec)
    }

    fn offset_faces(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        faces: &[mitcad_model::FaceName],
        distance: f64,
    ) -> Result<MockShape, KernelError> {
        self.hook("offset_faces");
        if !self.face_listing {
            return Err(KernelError::Unsupported("offset faces"));
        }
        self.mock.offset_faces(feature, body, faces, distance)
    }

    fn face_cylinder(
        &self,
        shape: &MockShape,
        face: &mitcad_model::FaceName,
    ) -> Result<mitcad_model::Cylinder, KernelError> {
        if !self.face_listing {
            return Err(KernelError::Unsupported("face cylinders"));
        }
        self.mock.face_cylinder(shape, face)
    }

    fn face_point_normal(
        &self,
        shape: &MockShape,
        face: &mitcad_model::FaceName,
        _near: [f64; 3],
    ) -> Result<mitcad_model::datum::SurfacePoint, KernelError> {
        if !self.face_listing || !shape.faces.contains(face) {
            return Err(KernelError::Unsupported("face normals"));
        }
        Ok(mitcad_model::datum::SurfacePoint {
            point: face_point(face),
            normal: [1.0, 0.0, 0.0],
        })
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

/// A circle of 10 mm diameter on XY (cm in the IR).
fn circle_sketch(index: i64) -> Value {
    json!({
        "index": index, "name": format!("Sketch{}", index + 1), "objectType": "Sketch",
        "detail": {
            "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
            "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
            "points": [{"id": "p0", "xyz": [2.0, 1.0, 0.0]}],
            "curves": [{"id": "c0", "type": "SketchCircle", "centerSketchPoint": "p0", "radius": 0.5}]
        }
    })
}

/// A thread item as the decoder gives it: its size, flags and the face
/// found in the history (its cylinder's axis as the file has it, `axis_z`
/// along z; a point on it, cm).
fn thread_item(index: i64, info: Value, full: bool, axis_z: f64, point: [f64; 3]) -> Value {
    json!({
        "index": index, "name": format!("Thread{index}"), "objectType": "ThreadFeature",
        "detail": {
            "threadInfo": info, "isModeled": false, "isFullLength": full,
            "threadLength": param("d4", "12 mm", 1.2, "mm"),
            "threadOffset": param("d5", "1 mm", 0.1, "mm"),
            "threadLocation": "HighEndThreadLocation",
            "inputCylindricalFaces": [{
                "kind": "face", "objectType": "BRepFace",
                "geometry": {"type": "Cylinder", "origin": [2.0, 1.0, 0.0],
                             "axis": [0.0, 0.0, axis_z], "radius": 0.5},
                "point_on_face": point,
                "_f3d": {"recipe": "bounded_face", "found": "before state 2"}
            }]
        }
    })
}

/// Threads (mitcad#35) on the faces the decoder found in the history: the
/// size as Mitcad lists it, a partial thread from the end the file's axis
/// points to (here against the mock cylinder's), a class the face's side
/// does not take left out, pipe threads by Mitcad's names.
#[test]
fn threads_come_in_on_the_faces_the_decoder_found() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut base = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(base.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let point = face_point(&side).map(|x| x / 10.0);
    let iso = |class: &str| {
        json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
               "threadDesignation": "M10x1.5", "threadClass": class, "isInternal": false})
    };
    let pipe = json!({"_type": "ThreadInfo", "threadType": "BSP Pipe Threads",
                      "threadDesignation": "G 1/4-19", "threadClass": "A"});
    base.push(thread_item(2, iso("6g"), false, -1.0, point));
    base.push(thread_item(3, iso("6H"), true, 1.0, point));
    base.push(thread_item(4, pipe, true, 1.0, point));
    let mut doc = Document::new(kernel());
    let report = import_design(
        &mut doc,
        &timeline_dump(base),
        &mut NoGeometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes[2..], [Outcome::Parametric; 3], "{}", report.text());
    let def = |uid: u64| -> Value {
        serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "F{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone()
    };
    let partial = def(3);
    assert_eq!(partial["faces"][0]["face"], side.to_string());
    assert_eq!(
        partial["thread"],
        json!({"designation": "M10x1.5", "class": "6g"})
    );
    // The length a parameter of the thread's own, the offset the file's d5.
    assert!(partial["length"].is_string(), "{partial}");
    assert_eq!(partial["offset"], "d5");
    // The file's axis runs down, the mock cylinder's up: the high end in
    // the file is Mitcad's low end.
    assert_eq!(partial["location"], "low_end");
    let full = def(4);
    assert_eq!(full["thread"], json!({"designation": "M10x1.5"}));
    assert!(full.get("length").is_none());
    assert!(
        report.items[3]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("class")),
        "{}",
        report.text()
    );
    assert_eq!(
        def(5)["thread"],
        json!({"standard": "whitworth", "designation": "G 1/4", "class": "A"})
    );
}

/// A modelled thread (mitcad#57) comes in with the file's diameters: the
/// thread alone first (it sizes its part of the face itself). Mitcad
/// models only the 60-degree threads: a modelled pipe thread falls back.
#[test]
fn modelled_threads_take_the_files_diameters() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let point = face_point(&side).map(|x| x / 10.0);
    let info = json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
        "threadDesignation": "M10x1.5", "threadClass": "6g", "isInternal": false,
        "majorDiameter": 0.985, "minorDiameter": 0.8141, "pitchDiameter": 0.8928});
    let mut modelled = thread_item(2, info, true, 1.0, point);
    modelled["detail"]["isModeled"] = json!(true);
    items.push(modelled);
    let pipe = json!({"_type": "ThreadInfo", "threadType": "BSP Pipe Threads",
                      "threadDesignation": "G 1/4-19", "threadClass": "A"});
    let mut modelled_pipe = thread_item(3, pipe, true, 1.0, point);
    modelled_pipe["detail"]["isModeled"] = json!(true);
    items.push(modelled_pipe);
    let mut doc = Document::new(kernel());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str::<Value>(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap())
            .unwrap()["def"]
            .clone();
    assert_eq!(def["type"], "thread", "{def}");
    assert_eq!(def["modeled"], true, "{def}");
    let spec = doc.kernel().mock.specs.borrow().join("\n");
    assert!(
        spec.contains(&format!(
            "thread {side} pitch 1.5 diameters 9.8500/8.1410/8.9280"
        )),
        "{spec}"
    );
    // Without a history the angle is not measured.
    assert!(def.get("angle").is_none(), "{def}");
    assert_ne!(report.items[3].outcome, Outcome::Parametric);
    assert!(
        report.items[3]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("Whitworth")),
        "{}",
        report.text()
    );
}

/// A thread sizes its face to the thread's major diameter first, as the
/// file does (mitcad#35); the thread keeps the item's name.
#[test]
fn a_thread_sizes_its_face_first() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let info = json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
        "threadDesignation": "M10x1.5", "threadClass": "6g", "isInternal": false,
        "majorDiameter": 0.985, "minorDiameter": 0.8141});
    items.push(thread_item(
        2,
        info,
        true,
        1.0,
        face_point(&side).map(|x| x / 10.0),
    ));
    let mut doc = Document::new(kernel());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert!(
        report.items[2]
            .note
            .as_deref()
            .is_some_and(|n| n.contains("sized")),
        "{}",
        report.text()
    );
    let offset = doc.feature(FeatureUid(3)).unwrap();
    assert!(
        matches!(offset.def, FeatureDef::OffsetFace(_)),
        "{offset:?}"
    );
    let thread = doc.feature(FeatureUid(4)).unwrap();
    assert!(matches!(thread.def, FeatureDef::Thread(_)));
    assert_eq!(thread.name, "Thread2");
}

/// A tapped hole (mitcad#35): the hole, and a thread on its wall from the
/// hole's start.
#[test]
fn a_tapped_hole_comes_in_with_its_thread_on_the_wall() {
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let top = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "end")
        .cloned()
        .expect("a top face");
    items.push(json!({
        "index": 2, "name": "Hole1", "objectType": "HoleFeature",
        "detail": {
            "holeType": "SimpleHoleType", "holeTapType": "TappedHoleTapType",
            "holeDiameter": param("d6", "5 mm", 0.5, "mm"),
            "tipAngle": param("d7", "118 deg", 2.0594885173533086, "deg"),
            "extentDefinition": {"_type": "DistanceExtentDefinition",
                                 "distance": param("d8", "15 mm", 1.5, "mm")},
            "position": [2.0, 1.0, 1.0],
            "holePositionDefinition": {"planarEntity": {
                "kind": "face", "geometry": {"type": "Plane"},
                "point_on_face": face_point(&top).map(|x| x / 10.0)}},
            "tappedHoleInfo": {"_type": "ThreadInfo", "threadType": "ISO Metric profile",
                               "threadDesignation": "M6x1", "threadClass": "6H",
                               "isInternal": true, "minorDiameter": 0.5035},
            "thread": {"_type": "ThreadFeature", "isModeled": false, "isFullLength": false,
                       "threadLength": param("d9", "10 mm", 1.0, "mm"),
                       "threadOffset": param("d10", "0 mm", 0.0, "mm")}
        }
    }));
    let mut doc = Document::new(kernel());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def = |uid: &str| -> Value {
        serde_json::from_str::<Value>(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .unwrap(),
        )
        .unwrap()["def"]
            .clone()
    };
    let hole = def("F3");
    assert_eq!(hole["type"], "hole");
    assert!(hole.get("thread").is_none(), "{hole}");
    let thread = def("F4");
    assert_eq!(thread["faces"][0]["face"], "F3:hole0.wall", "{thread}");
    assert_eq!(
        thread["thread"],
        json!({"designation": "M6x1", "class": "6H"})
    );
    assert_eq!(thread["location"], "low_end");
    // The bore is the hole's diameter (its own state's); the minor
    // diameter is the next guess.
    let params = doc.parameters();
    let bore = hole["diameter"].as_str().and_then(|n| params.find(n));
    assert_eq!(
        bore.and_then(|id| params.get(id)).map(|p| p.value()),
        Some(5.0),
        "{hole}"
    );
}

/// Sheets (mitcad#35): an item the import does not translate made a surface
/// body; it comes in with the item's fallback, and a later join leaves it
/// alone (its participants are the solids).
#[test]
fn sheets_of_the_history_come_in_as_stored_bodies() {
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "Patch1", "objectType": "PatchFeature", "detail": {}}),
        extrude(3, 0, "JoinFeatureOperation", "d2"),
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
    let joined = shapes(&reference);
    let sheet = MockShape {
        // The mock kernel's surface body.
        history: "patch,sheet".to_owned(),
        faces: block[0].faces[..1].to_vec(),
        edges: Vec::new(),
        bounds: None,
        parts: Vec::new(),
    };
    let mut geometry = TestGeometry {
        states: vec![
            Vec::new(),
            block.clone(),
            vec![block[0].clone(), sheet.clone()],
            vec![joined[0].clone(), sheet],
        ],
        items: HashMap::from([(1, 1), (2, 2), (3, 3)]),
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
    // The patch's fallback holds the sheet; the join works on the block.
    let FeatureDef::Base(base) = &doc.feature(FeatureUid(3)).unwrap().def else {
        panic!("a base feature");
    };
    assert_eq!(base.bodies.len(), 1);
    let FeatureDef::Extrude(join) = &doc.feature(FeatureUid(4)).unwrap().def else {
        panic!("an extrusion");
    };
    assert_eq!(
        join.participants,
        [mitcad_model::BodyUid::new(FeatureUid(2), 0)]
    );
    assert_eq!(report.items[3].verified, Some(true));
    let kinds: Vec<BodyKind> = doc
        .bodies()
        .iter()
        .map(|b| doc.kernel().body_kind(b.shape).unwrap())
        .collect();
    assert_eq!(kinds, [BodyKind::Solid, BodyKind::Sheet]);
    // The sheet is the file's: not an extra body, and no last fallback.
    assert!(report.extra_bodies.is_empty(), "{}", report.text());
    assert_eq!(doc.features().count(), 4, "{}", report.text());
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

#[test]
fn items_tried_in_vain_take_their_states_at_once_in_a_try_run_again() {
    // A first try (the kernel hung on item 1 before): the join's
    // definitions give no state (the mock kernel's join on the base
    // feature's body), which the progress keeps with why.
    use std::sync::atomic::Ordering;
    let run = |failed_items: Vec<(i64, String)>| {
        let (items, mut geometry) = block_history(Vec::new());
        let mut doc = Document::new(TestKernel::default());
        let progress = Arc::new(crate::Progress::new());
        let options = Options {
            hung_items: vec![1],
            failed_items,
            progress: Some(progress.clone()),
            ..Options::default()
        };
        let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
        (report, progress)
    };
    let (first, progress) = run(Vec::new());
    let failed = progress.failed_items();
    assert_eq!(
        failed.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
        [2],
        "{}",
        first.text()
    );
    let steps = progress.step.load(Ordering::Relaxed);
    // Run again with them: the join takes its state with the same note,
    // without its definitions being tried (fewer steps), and is not kept
    // again.
    let (second, progress) = run(failed);
    let outcomes = |r: &DesignReport| r.items.iter().map(|i| i.outcome).collect::<Vec<_>>();
    assert_eq!(outcomes(&second), outcomes(&first), "{}", second.text());
    assert_eq!(
        second.items[2].note,
        first.items[2].note,
        "{}",
        second.text()
    );
    assert!(progress.step.load(Ordering::Relaxed) < steps);
    assert!(progress.failed_items().is_empty());
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
fn bodies_the_decoder_named_are_found_by_points_on_their_edges() {
    // A block, then a move of it, the body given as the decoder gives it:
    // names and middle points of some of its edges (cm).
    let first = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel {
        edge_points: true,
        ..TestKernel::default()
    });
    import_design(
        &mut reference,
        &timeline_dump(first.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (uid, shape) = (reference.bodies()[0].uid, shapes(&reference)[0].clone());
    let points: Vec<[f64; 3]> = shape
        .edges
        .iter()
        .take(3)
        .map(|(a, b)| edge_point(a, b).map(|x| x / 10.0))
        .collect();
    let body = |points: Value| {
        json!({"kind": "body", "objectType": "BRepBody",
               "_f3d": {"recipe": "body", "entities": [[{"tag": "304", "kind": 7, "ops": []}]],
                        "edge_points": points}})
    };
    let replay = |body: Value| {
        let mut items = first.clone();
        let transform = json!([
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0, 0.5],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0]
        ]);
        let detail = json!({"inputEntities": [body], "transform": transform});
        let item = json!({"index": 2, "name": "Move1", "objectType": "MoveFeature",
                          "detail": detail});
        items.push(item);
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
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
    let (report, def) = replay(body(json!(points)));
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def = def.unwrap();
    assert_eq!(def["bodies"], json!([uid.to_string()]));
    assert_eq!(def["transform"]["matrix"][0][3], json!(10.0));
    // A point on no edge of the replay: not found.
    let (report, _) = replay(body(json!([[123.0, 0.0, 0.0]])));
    assert_ne!(report.items[2].outcome, Outcome::Parametric);
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
        // The occurrence of a component of another document: an empty
        // component stands for it (mitcad#75).
        assert_eq!(
            names,
            ["Plate:1", "Pin:1", "Plate:2", "Inserted component:1"]
        );
        let inserted = a.occurrences[3].component;
        assert!(doc.component_bodies(inserted).is_empty());
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

/// The block's sketch and extrusion owned by the given components
/// (`_f3d.component`, the decoder's), in a design with one occurrence of
/// Plate (object 10) placed 5 cm along X.
fn owned_dump(sketch_owner: u64, extrude_owner: u64) -> Dump {
    let mut value: Value = serde_json::to_value(block_dump()).unwrap();
    value["document"] = json!({"root_component": "Assembly"});
    value["components"] = json!([
        {"name": "Assembly", "is_root": true, "_f3d": {"object_id": 3}},
        {"name": "Plate", "_f3d": {"object_id": 10}}]);
    value["occurrences"] = json!([
        {"component": "Plate", "_f3d": {"component_object": 10},
         "transform": [[1.0, 0.0, 0.0, 5.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                       [0.0, 0.0, 0.0, 1.0]]}]);
    value["timeline"]["items"][0]["_f3d"] = json!({"component": sketch_owner});
    value["timeline"]["items"][1]["_f3d"]["component"] = json!(extrude_owner);
    Dump::from_json(&value.to_string()).unwrap()
}

#[test]
fn items_go_into_the_components_that_own_them() {
    // Without a history the decoder's owners place the items: both in
    // Plate (before mitcad#37 they went into the root).
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(
        &mut doc,
        &owned_dump(10, 10),
        &mut NoGeometry,
        &Options::default(),
    );
    let plate = doc.assembly().components[0].uid;
    assert!(
        doc.features().all(|f| f.component == plate),
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].outcome, Outcome::Parametric);
    assert_eq!(report.components.items.get("Plate"), Some(&2));

    // A sketch of the root used by Plate's extrusion: its geometry is in
    // the root's coordinates, so it stays there, and the extrusion uses a
    // copy moved into Plate's (5 cm back along X).
    let mut doc = Document::new(MockKernel::default());
    let report = import_design(
        &mut doc,
        &owned_dump(3, 10),
        &mut NoGeometry,
        &Options::default(),
    );
    assert_eq!(
        doc.feature(FeatureUid(1)).unwrap().component,
        ComponentUid::ROOT
    );
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let extrude: FeatureUid = report.items[1]
        .features
        .iter()
        .filter_map(|f| f.parse().ok())
        .find(|f| {
            matches!(
                doc.feature(*f).map(|e| &e.def),
                Some(FeatureDef::Extrude(_))
            )
        })
        .expect("an extrusion");
    assert_eq!(doc.feature(extrude).unwrap().component, plate);
    let def: Value = serde_json::from_str(
        &doc.query(&format!(r#"{{"query": "feature", "uid": "{extrude}"}}"#))
            .unwrap(),
    )
    .unwrap();
    let copy: FeatureUid = def["def"]["profiles"][0]["sketch"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_ne!(copy, FeatureUid(1));
    assert_eq!(doc.feature(copy).unwrap().component, plate);
    let origin = doc.sketch_output(copy).unwrap().frame.origin;
    assert!((origin[0] + 50.0).abs() < 1e-9, "{origin:?}");
}

/// [`owned_dump`] with Plate's occurrence stored `stored` cm along X and
/// a captured position at timeline index `captured` putting it 20 cm
/// along X; the extrusion is at index 1, or 2 after a captured position
/// at 1.
fn captured_dump(sketch_owner: u64, stored: f64, captured: i64) -> Dump {
    let mut value: Value = serde_json::to_value(owned_dump(sketch_owner, 10)).unwrap();
    value["occurrences"][0]["_f3d"]["object_id"] = json!(100);
    value["occurrences"][0]["transform"][0][3] = json!(stored);
    let position = json!({"index": captured, "name": "Position1", "objectType": "Snapshot",
        "_f3d": {"component": 3},
        "detail": {"positions": [{
            "occurrence": {"kind": "occurrence", "_f3d": {"path": [100], "context_component": 3}},
            "transform": [[1.0, 0.0, 0.0, 20.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0],
                          [0.0, 0.0, 0.0, 1.0]]}]}});
    let items = value["timeline"]["items"].as_array_mut().unwrap();
    if captured == 1 {
        items[1]["index"] = json!(2);
        items.insert(1, position);
        for p in value["parameters"]["model"].as_array_mut().unwrap() {
            if p["createdBy"]["timeline_index"] == json!(1) {
                p["createdBy"]["timeline_index"] = json!(2);
            }
        }
    } else {
        items.push(position);
    }
    Dump::from_json(&value.to_string()).unwrap()
}

/// The X of the origin of the sketch Plate's extrusion uses, and the
/// component it is in.
fn extruded_sketch(doc: &Document<MockKernel>, report: &DesignReport) -> (f64, ComponentUid) {
    let extrude = report
        .items
        .iter()
        .flat_map(|i| &i.features)
        .filter_map(|f| f.parse::<FeatureUid>().ok())
        .find(|f| {
            matches!(
                doc.feature(*f).map(|e| &e.def),
                Some(FeatureDef::Extrude(_))
            )
        })
        .expect("an extrusion");
    let def: Value = serde_json::from_str(
        &doc.query(&format!(r#"{{"query": "feature", "uid": "{extrude}"}}"#))
            .unwrap(),
    )
    .unwrap();
    let sketch: FeatureUid = def["def"]["profiles"][0]["sketch"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let origin = doc.sketch_output(sketch).unwrap().frame.origin;
    (origin[0], doc.feature(sketch).unwrap().component)
}

/// Items before a captured position were made with the occurrences where
/// they were before it (mitcad#86): a sketch of the root used by Plate's
/// extrusion is moved into Plate by the placements at the extrusion's
/// point of the timeline, although Plate starts at its last captured
/// position (mitcad#75).
#[test]
fn sketches_move_by_the_placements_at_their_users() {
    let run = |dump: &Dump| {
        let mut doc = Document::new(MockKernel::default());
        let report = import_design(&mut doc, dump, &mut NoGeometry, &Options::default());
        assert_eq!(report.joints.positions, 1, "{}", report.text());
        let extrusion = report
            .items
            .iter()
            .find(|i| i.object_type == "ExtrudeFeature")
            .unwrap();
        assert_eq!(extrusion.outcome, Outcome::Parametric, "{}", report.text());
        // Plate starts at its captured position in every case.
        let plate = &doc.assembly().occurrences[0];
        assert_eq!(plate.transform.translation[0], 200.0);
        let (x, component) = extruded_sketch(&doc, &report);
        (x, component == plate.component)
    };
    // Captured after the extrusion: Plate was at its stored 5 cm.
    assert_eq!(run(&captured_dump(3, 5.0, 2)), (-50.0, true));
    // Captured before it: at the captured 20 cm.
    assert_eq!(run(&captured_dump(3, 5.0, 1)), (-200.0, true));
    // Plate stored where the root is: the sketch follows the extrusion into
    // Plate as it is, unless a captured position before the extrusion had
    // moved Plate away (a copy moved by 20 cm then).
    assert_eq!(run(&captured_dump(3, 0.0, 2)), (0.0, true));
    assert_eq!(run(&captured_dump(3, 0.0, 1)), (-200.0, true));
}

#[test]
fn an_item_without_a_state_leaves_the_next_items_state_to_it() {
    // Two extrusions of the same block; the history names the second one's
    // state, so the first one (whose operation the file does not have:
    // rolled back, say) does not take it.
    let items = vec![
        rectangle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        extrude(2, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(TestKernel::default());
    import_design(
        &mut reference,
        &timeline_dump(items[..2].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        items: HashMap::from([(2, 1)]),
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
        outcomes,
        [Outcome::Partial, Outcome::Skipped, Outcome::Parametric],
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
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

/// The stream decoder's sweeps, pipes and lofts (mitcad#34): profiles by
/// their sketch only, a second fraction 0, no direction, orientation,
/// section type or wall. Without a history the first reading is taken: a
/// region of the sketch, the whole path, a solid circular pipe.
#[test]
fn sweeps_pipes_and_lofts_of_the_stream_decoder() {
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
    let line = json!([{"_type": "PathEntity", "entity": {"kind": "sketch_entity",
        "objectType": "SketchLine", "sketch": "Sketch2", "sketch_timeline_index": 1, "id": "c0"}}]);
    let profile = json!({"kind": "profile", "sketch": "Sketch1", "sketch_timeline_index": 0});
    let sweep = json!({"index": 2, "name": "Sweep1", "objectType": "SweepFeature", "detail": {
        "profile": [profile], "operation": "NewBodyFeatureOperation", "path": line,
        "distanceOne": param("d7", "1", 1.0, ""), "distanceTwo": param("d8", "0", 0.0, ""),
        "twistAngle": param("d9", "0 deg", 0.0, "deg"),
        "taperAngle": param("d10", "0 deg", 0.0, "deg")}});
    let pipe = json!({"index": 3, "name": "Pipe1", "objectType": "PipeFeature", "detail": {
        "path": line, "sectionSize": param("d11", "8 mm", 0.8, "mm"),
        "sectionThickness": param("d12", "1 mm", 0.1, "mm"),
        "distanceOne": param("d13", "1", 1.0, ""), "distanceTwo": param("d14", "0", 0.0, ""),
        "operation": "NewBodyFeatureOperation"}});
    let loft = json!({"index": 4, "name": "Loft1", "objectType": "LoftFeature", "detail": {
        "operation": "NewBodyFeatureOperation",
        "loftSections": [
            {"_type": "LoftSection", "index": 0, "entity": profile,
             "endCondition": {"_type": "LoftFreeEndCondition"}},
            {"_type": "LoftSection", "index": 1, "entity": {"kind": "sketch_entity",
             "objectType": "SketchPoint", "sketch": "Sketch2", "sketch_timeline_index": 1, "id": "p1"},
             "endCondition": {"_type": "LoftPointTangentEndCondition",
                              "weight": param("d15", "1.5", 1.5, "")}}],
        "centerLineOrRails": [], "centerLineOrRails.isCenterLine": false}});
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
    assert_eq!(sweep["profiles"][0]["sketch"], "F1");
    assert_eq!(sweep["path"]["sketch"], "F2");
    for key in ["extent", "flip", "orientation"] {
        assert!(sweep.get(key).is_none(), "{key}: {sweep}");
    }
    let pipe = def(4);
    for key in ["section", "thickness", "extent"] {
        assert!(pipe.get(key).is_none(), "{key}: {pipe}");
    }
    let loft = def(5);
    assert_eq!(loft["sections"][0]["type"], "profile");
    assert_eq!(loft["sections"][1]["type"], "point");
    assert_eq!(loft["sections"][1]["point"]["sketch"], "F2");
    assert_eq!(loft["end_condition"]["type"], "point_tangent");
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
fn joins_that_remove_material_are_not_taken() {
    let sig = |volume: f64| Sig {
        volume,
        area: 10.0,
        center: [0.0; 3],
        faces: 6,
    };
    let feature = |operation: &str| {
        Candidate::new(json!({"type": "extrude", "operation": operation, "extent": {}}))
    };
    let (join, cut) = (feature("join"), feature("cut"));
    assert!(implausible(&join, &[sig(100.0)], &[sig(99.0)]));
    assert!(!implausible(&join, &[sig(100.0)], &[sig(101.0)]));
    assert!(implausible(&cut, &[sig(100.0)], &[sig(101.0)]));
    assert!(!implausible(&cut, &[sig(100.0)], &[sig(99.0)]));
    // Bodies joined into one hold their overlap once.
    assert!(!implausible(&join, &[sig(100.0), sig(50.0)], &[sig(140.0)]));
}

/// The smaller of the two rectangles' regions in Sketch1 (F1), 10 x 20 mm.
const SMALL: &str = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";

/// A document with Sketch1 of the two rectangles (F1) and the features
/// `defs` after it, made by the plain test kernel: the file's bodies.
fn reference(defs: Vec<Value>) -> Document<TestKernel> {
    let mut doc = Document::new(TestKernel::default());
    import_design(
        &mut doc,
        &timeline_dump(vec![two_rectangles(0)]),
        &mut NoGeometry,
        &Options::default(),
    );
    for def in defs {
        add(&mut doc, def);
    }
    doc
}

fn extrusion_of(region: &str) -> Value {
    json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
           "extent": {"type": "distance", "distance": 10}, "operation": "new_body"})
}

#[test]
fn an_approximate_extrusion_does_not_carry_its_difference() {
    // Mitcad's extrusions come out 3e-4 larger than the file's (as regions
    // next to the file's would): the closest one is kept, and a base
    // feature brings the bodies to the file's state after it, so that the
    // final bodies are the stored ones exactly.
    let items = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let file = reference(vec![extrusion_of(SMALL)]);
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&file)],
        items: HashMap::from([(1, 1)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel {
        skew: true,
        ..TestKernel::default()
    });
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[1].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert_eq!(report.items[1].verified, Some(true));
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["profiles"][0]["region"], SMALL);
    // The base feature after it, and the stored body exactly.
    assert!(
        matches!(
            doc.feature(FeatureUid(3)).map(|f| &f.def),
            Some(FeatureDef::Base(_))
        ),
        "{}",
        report.text()
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("brings the bodies to the file's state after Extrude1")),
        "{}",
        report.text()
    );
    assert_eq!(report.bodies.len(), 1);
    assert_eq!(report.bodies[0].volume_difference, Some(0.0));
    // A difference the bodies carry from an approximate item before is not
    // the item's own.
    assert!(introduced(3e-4, 0.0) && !introduced(3e-4, 3e-4) && introduced(6e-4, 3e-4));
    assert!(!introduced(5e-6, 0.0));
    // A closer extrusion takes an approximate one's place; a fillet only
    // when it comes closer by a quarter.
    let fillet = Candidate::new(json!({"type": "fillet", "body": "F2.b0", "sets": []}));
    let extrude = Candidate::new(json!({"type": "extrude"}));
    assert!(closer(&extrude, 4.0e-4, 4.4e-4) && !closer(&fillet, 4.0e-4, 4.4e-4));
    assert!(closer(&fillet, 5.2e-6, 1.0e-5) && !closer(&extrude, 5e-4, 4.4e-4));
}

#[test]
fn a_pattern_gets_the_extrusion_profiles_it_copies() {
    // Both rectangles extruded give the same body as far as the history
    // tells (the test kernel measures every body alike), but only the
    // file's region gives the pattern's copies: the extrusion takes the
    // first, and the pattern after it changes it to the file's.
    let items = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
        json!({"index": 2, "name": "C-Pattern1", "objectType": "CircularPatternFeature",
               "detail": {"quantity": param("d6", "2", 2.0, "")}}),
    ];
    let file = reference(vec![
        extrusion_of(SMALL),
        json!({"type": "circular_pattern", "objects": {"type": "features", "features": ["F2"]},
               "axis": "z", "quantity": 2, "angle": std::f64::consts::TAU, "symmetric": false,
               "compute": "adjust"}),
    ]);
    let extruded = file.bodies()[..1].iter().map(|b| b.shape.clone()).collect();
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), extruded, shapes(&file)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel {
        alike: true,
        ..TestKernel::default()
    });
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [
            Outcome::Parametric,
            Outcome::Parametric,
            Outcome::Parametric
        ],
        "{}",
        report.text()
    );
    assert_eq!(report.items[2].verified, Some(true));
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F2"}"#).unwrap()).unwrap();
    assert_eq!(def["def"]["profiles"][0]["region"], SMALL);
    // (The large rectangle was the first guess.)
    assert!(
        report.items[1]
            .note
            .as_deref()
            .unwrap_or_default()
            .contains("chosen for C-Pattern1"),
        "{}",
        report.text()
    );
}

#[test]
fn a_rectangular_pattern_without_directions_finds_a_hexagonal_lattice() {
    // The oldest item version stores no directions (mitcad#74): the file's
    // pattern repeats the extrusion along x and at 60° to it, on both sides.
    let lattice = json!({"type": "rectangular_pattern",
        "objects": {"type": "features", "features": ["F2"]},
        "direction1": {"axis": "x", "quantity": 2, "distance": 30, "symmetric": true},
        "direction2": {"axis": {"origin": [0.0, 0.0, 0.0],
                                "direction": [0.5, 3f64.sqrt() / 2.0, 0.0]},
                       "quantity": 2, "distance": 30, "symmetric": true},
        "distance_type": "spacing", "original_bodies": true});
    let file = reference(vec![extrusion_of(SMALL), lattice]);
    let extruded = file.bodies()[..1].iter().map(|b| b.shape.clone()).collect();
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), extruded, shapes(&file)],
        items: HashMap::from([(1, 1), (2, 2)]),
        ..TestGeometry::default()
    };
    let distance = param("d9", "30 mm", 3.0, "mm");
    let items = vec![
        two_rectangles(0),
        json!({"index": 1, "name": "Extrude1", "objectType": "ExtrudeFeature",
               "detail": {"operation": "NewBodyFeatureOperation",
                          "profile": [{"kind": "profile", "sketch": "Sketch1",
                                       "sketch_timeline_index": 0}],
                          "extentType": "OneSideFeatureExtentType",
                          "extentOne": {"_type": "DistanceExtentDefinition",
                                        "distance": param("d2", "10 mm", 1.0, "mm")}}}),
        json!({"index": 2, "name": "R-Pattern1", "objectType": "RectangularPatternFeature",
               "detail": {"inputEntities": [{"kind": "feature", "objectType": "ExtrudeFeature",
                                             "name": "Extrude1", "timeline_index": 1}],
                          "patternEntityType": "FeaturesPatternType",
                          "quantityOne": param("d7", "2", 2.0, ""),
                          "quantityTwo": param("d8", "2", 2.0, ""),
                          "distanceOne": distance, "distanceTwo": distance}}),
    ];
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    let def: Value =
        serde_json::from_str(&doc.query(r#"{"query": "feature", "uid": "F3"}"#).unwrap()).unwrap();
    let def = &def["def"];
    assert_eq!(def["direction1"]["symmetric"], json!(true), "{def}");
    let across = def["direction2"]["axis"]["direction"][1]
        .as_f64()
        .unwrap_or_default();
    assert!((across - 3f64.sqrt() / 2.0).abs() < 1e-9, "{def}");
    assert_eq!(def["original_bodies"], json!(true), "{def}");
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
fn edges_the_decoder_found_by_their_names_come_first() {
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
    let names = json!({"recipe": "edge", "entities": [[{"tag": "3", "kind": 0, "ops": [301]}],
                       [{"tag": "4", "kind": 0, "ops": [301]}]]});
    let replay = |edge: Value| {
        let mut items = block_items.clone();
        let set = json!({"radius": param("d5", "2 mm", 0.2, "mm"), "edges": [edge]});
        let fillet = json!({"index": 2, "name": "Fillet1", "objectType": "FilletFeature",
                            "detail": {"edgeSets": [set]}});
        items.push(fillet);
        let mut doc = Document::new(TestKernel {
            edge_points: true,
            ..TestKernel::default()
        });
        import_design(
            &mut doc,
            &timeline_dump(items),
            &mut NoGeometry,
            &Options::default(),
        )
    };
    // Found in the history: the decoder wrote its geometry (cm).
    let report = replay(json!({"kind": "edge", "length": 0.1,
        "mid_point": [p[0] / 10.0, p[1] / 10.0, p[2] / 10.0],
        "_f3d": {"found": "BREP.x.smbh before state 4", "recipe": "edge", "entities": names["entities"]}}));
    let item = &report.items[2];
    assert_eq!(item.outcome, Outcome::Parametric, "{}", report.text());
    assert_eq!(
        item.note.as_deref(),
        Some("edges found by their names in the file"),
        "{}",
        report.text()
    );
    // Not found: the edges are left to the history (here there is none).
    let report = replay(json!({"kind": "edge",
        "_f3d": {"found": "a face of the edge is not in the history state", "recipe": "edge",
                 "entities": names["entities"]}}));
    let item = &report.items[2];
    assert_ne!(item.outcome, Outcome::Parametric);
    assert!(
        report.text().contains("no history to find them"),
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

/// An extrusion of the stream decoder with its raw extent codes and
/// direction vector, without an extent of its own unless `distance`.
fn stream_extrude(index: i64, operation: &str, raw: Value, distance: Option<&str>) -> Value {
    let mut item = extrude(index, 0, operation, "d2");
    item["_f3d"]["extrude"] = raw;
    let detail = item["detail"].as_object_mut().unwrap();
    detail.remove("extentType");
    match distance {
        Some(d) => detail["extentOne"]["distance"] = param(d, "10 mm", 1.0, "mm"),
        None => {
            detail.remove("extentOne");
        }
    }
    item
}

#[test]
fn stream_extent_codes_and_direction_vector_choose_the_extent() {
    // Without a history only the first definition that is not a guess (of
    // the last item).
    let last = |items: Vec<Value>| {
        let n = items.len();
        let dump = timeline_dump(items);
        let mut doc = Document::new(MockKernel::default());
        let report = import(&mut doc, &dump);
        // (The sketch is partial: its polygon constraint is left out.)
        assert!(
            report.items[1..]
                .iter()
                .all(|i| i.outcome == Outcome::Parametric),
            "{}",
            report.text()
        );
        let def: Value = serde_json::from_str(
            &doc.query(&format!(r#"{{"query": "feature", "uid": "F{n}"}}"#))
                .unwrap(),
        )
        .unwrap();
        def["def"].clone()
    };
    let new_body = "NewBodyFeatureOperation";
    let first = |raw: Value, distance: Option<&str>| {
        last(vec![
            rectangle_sketch(0),
            stream_extrude(1, new_body, raw, distance),
        ])
    };
    // The vector against the sketch's normal flips it, whatever the first
    // ±1 of the stream says.
    let def = first(
        json!({"operation_code": 4, "extent_a": 1, "extent_b": 2, "direction": 1.0,
               "direction_vector": [0.0, 0.0, -1.0]}),
        Some("d2"),
    );
    assert_eq!(def["extent"]["type"], "distance");
    assert_eq!(def["flip"], true);
    let def = first(
        json!({"operation_code": 4, "extent_a": 1, "extent_b": 2, "direction": -1.0,
               "direction_vector": [0.0, 0.0, 1.0]}),
        Some("d2"),
    );
    assert_ne!(def["flip"], true);
    // Code 3: symmetric, a half each way.
    let def = first(
        json!({"operation_code": 4, "extent_a": 3, "extent_b": 2,
               "direction_vector": [0.0, 0.0, 1.0]}),
        Some("d2"),
    );
    assert_eq!(def["extent"]["type"], "symmetric");
    assert_ne!(def["extent"]["full_length"], true);
    // One side through all (code 0) of a block below the sketch: the
    // stored vector points away from the extrusion.
    let def = last(vec![
        rectangle_sketch(0),
        stream_extrude(
            1,
            new_body,
            json!({"operation_code": 4, "extent_a": 1, "extent_b": 2,
                   "direction_vector": [0.0, 0.0, -1.0]}),
            Some("d2"),
        ),
        stream_extrude(
            2,
            "CutFeatureOperation",
            json!({"operation_code": 2, "extent_a": 1, "extent_b": 0,
                   "direction_vector": [0.0, 0.0, 1.0]}),
            None,
        ),
    ]);
    assert_eq!(def["extent"]["type"], "through_all");
    assert_eq!(def["flip"], true);
}

// Polygons, sketch patterns, offsets and concentric circle dimensions
// (mitcad#36).

/// A dump of one sketch on XY with the given model parameters.
fn group_sketch_dump(
    params: Vec<Value>,
    points: Vec<Value>,
    curves: Vec<Value>,
    constraints: Value,
    dimensions: Value,
) -> Dump {
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "groups.f3d"},
        "parameters": {"model": params},
        "timeline": {"items": [{
            "index": 0, "name": "Sketch1", "objectType": "Sketch",
            "detail": {
                "referencePlane": {"kind": "construction_plane", "name": "XY", "origin": "XY"},
                "model_frame": {"sketch_to_model": [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0],
                                                    [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]]},
                "points": points, "curves": curves,
                "constraints": constraints, "dimensions": dimensions
            }
        }]}
    }))
}

fn gpoint(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "xyz": [x, y, 0.0]})
}

fn gfixed(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "xyz": [x, y, 0.0], "isFixed": true})
}

fn gline(id: &str, a: &str, b: &str) -> Value {
    json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b})
}

fn gcircle(id: &str, center: &str, radius: f64) -> Value {
    json!({"id": id, "type": "SketchCircle", "centerSketchPoint": center, "radius": radius})
}

fn sketch_query(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(&doc.query(r#"{"query": "sketch", "uid": "F1"}"#).unwrap()).unwrap()
}

/// The id of the point at `at` (sketch mm).
fn point_at(s: &Value, at: [f64; 2]) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "point"
                && (e["at"][0].as_f64().unwrap() - at[0]).abs() < 1e-6
                && (e["at"][1].as_f64().unwrap() - at[1]).abs() < 1e-6
        })
        .unwrap_or_else(|| panic!("a point at {at:?}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id}"))
}

fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

/// The circle whose centre is the point at `center` and whose radius is
/// `radius` (sketch mm).
fn circle_id(s: &Value, center: [f64; 2], radius: f64) -> String {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| {
            e["type"] == "circle"
                && e["center"]
                    .as_str()
                    .is_some_and(|c| (at(s, c)[0] - center[0]).abs() < 1e-6)
                && (e["geometry"]["radius"].as_f64().unwrap() - radius).abs() < 1e-6
        })
        .unwrap_or_else(|| panic!("a circle of {radius} at {center:?}"))["id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn radius(s: &Value, id: &str) -> f64 {
    entity(s, id)["geometry"]["radius"].as_f64().unwrap()
}

fn assert_parametric(report: &DesignReport) {
    assert_eq!(
        report.items[0].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
}

#[test]
fn a_polygon_stays_regular() {
    // A hexagon of radius 10 mm about the fixed origin, dimensioned centre
    // to corner.
    let corner = |i: usize| {
        let a = std::f64::consts::FRAC_PI_3 * i as f64;
        gpoint(&format!("p{i}"), a.cos(), a.sin())
    };
    let mut points: Vec<Value> = (0..6).map(corner).collect();
    points.push(gfixed("p6", 0.0, 0.0));
    let curves = (0..6)
        .map(|i| {
            gline(
                &format!("c{i}"),
                &format!("p{i}"),
                &format!("p{}", (i + 1) % 6),
            )
        })
        .collect();
    let dump = group_sketch_dump(
        vec![json!({"name": "d1", "expression": "10 mm", "value": 1.0, "unit": "mm"})],
        points,
        curves,
        json!([{"id": "k0", "type": "PolygonConstraint",
                "refs": {"entities": ["p0", "p1", "p2", "p3", "p4", "p5", "p6"]}}]),
        json!([{"id": "d0", "type": "SketchLinearDimension",
                "parameter": param("d1", "10 mm", 1.0, "mm"),
                "refs": {"entities": ["p6", "p0"]}}]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let corners: Vec<String> = (0..6)
        .map(|i| {
            let a = std::f64::consts::FRAC_PI_3 * i as f64;
            point_at(&s, [10.0 * a.cos(), 10.0 * a.sin()])
        })
        .collect();
    let center = point_at(&s, [0.0, 0.0]);
    // A construction circle through the corners and equal sides.
    let circles = s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["type"] == "circle" && e["construction"] == true)
        .count();
    assert_eq!(circles, 1);
    let kinds = |t: &str| {
        s["constraints"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["type"] == t)
            .count()
    };
    assert_eq!((kinds("coincident"), kinds("equal")), (6, 5));
    doc.set_parameter_expression("d1", "15 mm", None).unwrap();
    let s = sketch_query(&doc);
    let c = at(&s, &center);
    for i in 0..6 {
        let (a, b) = (at(&s, &corners[i]), at(&s, &corners[(i + 1) % 6]));
        assert!(((a[0] - c[0]).hypot(a[1] - c[1]) - 15.0).abs() < 1e-6);
        assert!(((a[0] - b[0]).hypot(a[1] - b[1]) - 15.0).abs() < 1e-6);
    }
}

#[test]
fn circular_pattern_copies_follow_their_original() {
    // A circle 20 mm from the fixed centre, and its copies at 120 and 240
    // degrees.
    let s3 = 3f64.sqrt();
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", 2.0, 0.0),
        gpoint("p2", -1.0, s3),
        gpoint("p3", -1.0, -s3),
    ];
    let curves = vec![
        gcircle("c0", "p1", 0.3),
        gcircle("c1", "p2", 0.3),
        gcircle("c2", "p3", 0.3),
    ];
    let tau = std::f64::consts::TAU;
    let dump = group_sketch_dump(
        vec![
            json!({"name": "d1", "expression": "20 mm", "value": 2.0, "unit": "mm"}),
            json!({"name": "d6", "expression": "3", "value": 3.0, "unit": ""}),
            json!({"name": "d7", "expression": "360 deg", "value": tau, "unit": "deg"}),
        ],
        points,
        curves,
        json!([{"id": "k0", "type": "CircularPatternConstraint",
                "refs": {"entities": ["c0", "c1", "c2", "p0"]},
                "props": {"centerPoint": "p0", "createdEntities": ["c1", "c2"],
                          "quantity": param("d6", "3", 3.0, ""),
                          "totalAngle": param("d7", "360 deg", tau, "deg")}}]),
        json!([{"id": "d0", "type": "SketchLinearDimension",
                "parameter": param("d1", "20 mm", 2.0, "mm"),
                "refs": {"entities": ["p0", "p1"]}}]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let pattern = &s["patterns"][0];
    assert_eq!(
        (pattern["type"].clone(), pattern["count"].clone()),
        (json!("circular"), json!(3))
    );
    assert_eq!(pattern["copies"].as_array().unwrap().len(), 2);
    let copy = point_at(&s, [-10.0, 10.0 * s3]);
    doc.set_parameter_expression("d1", "30 mm", None).unwrap();
    let s = sketch_query(&doc);
    let p = at(&s, &copy);
    assert!(
        (p[0] + 15.0).abs() < 1e-6 && (p[1] - 15.0 * s3).abs() < 1e-6,
        "{p:?}"
    );
}

#[test]
fn rectangular_pattern_with_a_negative_distance() {
    // A circle at the fixed origin and its copies 20 and 40 mm to the left
    // (direction +x, distance -20 mm).
    let curves = vec![
        gcircle("c0", "p0", 0.5),
        gcircle("c1", "p1", 0.5),
        gcircle("c2", "p2", 0.5),
    ];
    let params = vec![
        json!({"name": "d6", "expression": "3", "value": 3.0, "unit": ""}),
        json!({"name": "d7", "expression": "1", "value": 1.0, "unit": ""}),
        json!({"name": "d8", "expression": "-20 mm", "value": -2.0, "unit": "mm"}),
        json!({"name": "d9", "expression": "0 mm", "value": 0.0, "unit": "mm"}),
    ];
    let constraints = json!([{"id": "k0", "type": "RectangularPatternConstraint",
        "refs": {"entities": ["c0", "c1", "c2"]},
        "props": {"createdEntities": ["c1", "c2"],
                  "quantityOne": param("d6", "3", 3.0, ""),
                  "quantityTwo": param("d7", "1", 1.0, ""),
                  "distanceOne": param("d8", "-20 mm", -2.0, "mm"),
                  "distanceTwo": param("d9", "0 mm", 0.0, "mm"),
                  "directionOne": [1.0, 0.0, 0.0], "directionTwo": [0.0, 1.0, 0.0],
                  "flags": [0, 0, 0]}}]);
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", -2.0, 0.0),
        gpoint("p2", -4.0, 0.0),
    ];
    let dump = group_sketch_dump(
        params.clone(),
        points,
        curves.clone(),
        constraints.clone(),
        json!([]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let pattern = &s["patterns"][0];
    assert_eq!(pattern["direction"], json!([-1.0, 0.0]));
    assert_eq!(pattern["count"], json!([3, 1]));
    let far = point_at(&s, [-40.0, 0.0]);
    doc.set_parameter_expression("d8", "-30 mm", None).unwrap();
    let s = sketch_query(&doc);
    let p = at(&s, &far);
    assert!((p[0] + 60.0).abs() < 1e-6 && p[1].abs() < 1e-6, "{p:?}");

    // Copies on both sides of the original: not what Mitcad's pattern
    // makes, so it is left out.
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gpoint("p1", -2.0, 0.0),
        gpoint("p2", 2.0, 0.0),
    ];
    let dump = group_sketch_dump(params, points, curves, constraints, json!([]));
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_eq!(report.items[0].outcome, Outcome::Partial);
    assert!(
        report.items[0]
            .note
            .as_deref()
            .unwrap()
            .contains("symmetric")
    );
}

#[test]
fn offsets_keep_their_distance() {
    // A fixed 40 x 20 mm rectangle and its offset 5 mm inside (distance
    // parameter -5 mm); a circle of radius 10 mm and its offset 5 mm out,
    // and a third circle 5 mm outside that (a concentric circle dimension).
    let points = vec![
        gfixed("p0", 0.0, 0.0),
        gfixed("p1", 4.0, 0.0),
        gfixed("p2", 4.0, 2.0),
        gfixed("p3", 0.0, 2.0),
        gpoint("p4", 0.5, 0.5),
        gpoint("p5", 3.5, 0.5),
        gpoint("p6", 3.5, 1.5),
        gpoint("p7", 0.5, 1.5),
        gfixed("p8", 10.0, 0.0),
        gpoint("p9", 10.0, 0.0),
    ];
    let curves = vec![
        gline("c0", "p0", "p1"),
        gline("c1", "p1", "p2"),
        gline("c2", "p2", "p3"),
        gline("c3", "p3", "p0"),
        gline("c4", "p4", "p5"),
        gline("c5", "p5", "p6"),
        gline("c6", "p6", "p7"),
        gline("c7", "p7", "p4"),
        gcircle("c8", "p8", 1.0),
        gcircle("c9", "p9", 1.5),
        gcircle("c10", "p8", 2.0),
    ];
    let dump = group_sketch_dump(
        vec![
            json!({"name": "d1", "expression": "-5 mm", "value": -0.5, "unit": "mm"}),
            json!({"name": "d2", "expression": "5 mm", "value": 0.5, "unit": "mm"}),
            json!({"name": "d3", "expression": "5 mm", "value": 0.5, "unit": "mm"}),
            json!({"name": "d4", "expression": "20 mm", "value": 2.0, "unit": "mm"}),
        ],
        points,
        curves,
        json!([
            {"id": "k0", "type": "OffsetConstraint",
             "refs": {"entities": ["c0", "c4", "c1", "c5", "c2", "c6", "c3", "c7"]},
             "props": {"distance": -0.5, "dimension": "d0",
                       "parentCurves": ["c0", "c1", "c2", "c3"],
                       "childCurves": ["c4", "c5", "c6", "c7"]}},
            {"id": "k1", "type": "OffsetConstraint", "refs": {"entities": ["c8", "c9"]},
             "props": {"distance": 0.5, "dimension": "d1",
                       "parentCurves": ["c8"], "childCurves": ["c9"]}}
        ]),
        json!([
            {"id": "d0", "type": "SketchOffsetCurvesDimension",
             "parameter": param("d1", "-5 mm", -0.5, "mm"), "refs": {"entities": ["c0"]}},
            {"id": "d1", "type": "SketchOffsetCurvesDimension",
             "parameter": param("d2", "5 mm", 0.5, "mm"), "refs": {"entities": ["c8"]}},
            {"id": "d2", "type": "SketchConcentricCircleDimension",
             "parameter": param("d3", "5 mm", 0.5, "mm"), "refs": {"entities": ["c9", "c10"]},
             "textPosition": [10.0, 2.2, 0.0]},
            {"id": "d3", "type": "SketchDiameterDimension",
             "parameter": param("d4", "20 mm", 2.0, "mm"), "refs": {"entities": ["c8"]}}
        ]),
    );
    let mut doc = Document::new(MockKernel::default());
    let report = import(&mut doc, &dump);
    assert_parametric(&report);
    let s = sketch_query(&doc);
    let offsets = s["offsets"].as_array().unwrap();
    assert_eq!(offsets.len(), 2);
    // Inside the rectangle, which runs counter-clockwise: its left; the
    // distance is the magnitude of d1.
    assert_eq!(offsets[0]["left"], true);
    assert_eq!(offsets[0]["distance"], "d1_offset");
    assert_eq!(offsets[1].get("left"), None);
    assert_eq!(offsets[1]["distance"], "d2");
    let p = doc.parameters();
    let magnitude = p.get(p.find("d1_offset").unwrap()).unwrap();
    assert_eq!((magnitude.expression(), magnitude.value()), ("-(d1)", 5.0));
    let corner = point_at(&s, [5.0, 5.0]);
    let (c9, c10) = (
        circle_id(&s, [100.0, 0.0], 15.0),
        circle_id(&s, [100.0, 0.0], 20.0),
    );
    doc.set_parameter_expression("d1", "-8 mm", None).unwrap();
    doc.set_parameter_expression("d2", "7 mm", None).unwrap();
    let s = sketch_query(&doc);
    let p = at(&s, &corner);
    assert!(
        (p[0] - 8.0).abs() < 1e-6 && (p[1] - 8.0).abs() < 1e-6,
        "{p:?}"
    );
    assert!((radius(&s, &c9) - 17.0).abs() < 1e-6);
    assert!((radius(&s, &c10) - 22.0).abs() < 1e-6);
}

/// Items the decoder names without translating them (mitcad#43): those
/// that change no body are skipped at once with what they are; one that
/// changes bodies takes its history state with the reason.
#[test]
fn named_items_without_geometry_are_skipped_and_others_say_why_they_fall_back() {
    let (mut items, mut geometry) = block_history(Vec::new());
    let named = |index: i64, name: &str, t: &str| json!({"index": index, "name": name, "objectType": t, "detail": {}});
    items[2] = named(6, "EdgeFlange1", "FlangeFeature");
    items.splice(
        2..2,
        [
            named(2, "Pin", "Occurrence"),
            named(3, "GeometricRelationship1", "GeometricRelationship"),
            named(4, "Fixings", "Group"),
            named(5, "R-Pattern1", "RectangularOccurrencePattern"),
        ],
    );
    geometry.items = HashMap::from([(1, 1), (6, 2)]);
    let mut doc = Document::new(TestKernel::default());
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options::default(),
    );
    let got: Vec<(&str, Outcome)> = report.items[2..]
        .iter()
        .map(|i| (i.object_type.as_str(), i.outcome))
        .collect();
    assert_eq!(
        got,
        [
            ("Occurrence", Outcome::Skipped),
            ("GeometricRelationship", Outcome::Skipped),
            ("Group", Outcome::Skipped),
            ("RectangularOccurrencePattern", Outcome::Skipped),
            ("FlangeFeature", Outcome::Fallback),
        ],
        "{}",
        report.text()
    );
    let note = |k: usize| report.items[k].note.clone().unwrap_or_default();
    assert!(
        note(2).starts_with("no geometry: places occurrences"),
        "{}",
        note(2)
    );
    assert!(
        note(3).starts_with("no geometry: an assembly relationship"),
        "{}",
        note(3)
    );
    assert!(
        note(4).starts_with("no geometry: a timeline group"),
        "{}",
        note(4)
    );
    assert!(note(6).starts_with("a sheet metal flange"), "{}", note(6));
    assert!(report.history.reached_end, "{}", report.text());
}

/// The block's import (sketch, new body, join) on a thread of its own, its
/// first extrusion's kernel call running `hook`: the report and how many
/// extrusions the kernel was asked for.
fn import_block_on_thread(
    options: Options,
    hook: Box<dyn FnMut(&MockKernel) + Send>,
) -> std::thread::JoinHandle<(DesignReport, usize)> {
    std::thread::spawn(move || {
        let (items, mut geometry) = block_history(Vec::new());
        let kernel = TestKernel::default();
        let mut hook = Some(hook);
        let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&calls);
        *kernel.on_call.borrow_mut() = Some((
            "extrude_feature",
            Box::new(move |mock: &MockKernel| {
                counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Some(mut h) = hook.take() {
                    h(mock);
                }
            }),
        ));
        let mut doc = Document::new(kernel);
        let report = import_design(&mut doc, &timeline_dump(items), &mut geometry, &options);
        (report, calls.load(std::sync::atomic::Ordering::Relaxed))
    })
}

#[test]
fn a_try_the_watchdog_gave_up_does_no_more_items() {
    // The first extrusion's kernel call blocks past the hang limit (until
    // released); the watchdog gives the try up meanwhile (mitcad#71).
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    let progress = Arc::new(crate::Progress::new());
    let (entered, blocked) = mpsc::channel::<()>();
    let (release, released) = mpsc::channel::<()>();
    let worker = import_block_on_thread(
        Options {
            progress: Some(progress.clone()),
            ..Options::default()
        },
        Box::new(move |_| {
            entered.send(()).unwrap();
            released.recv().unwrap();
        }),
    );
    blocked
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap();
    // No step while the call blocks: the watchdog's hang.
    let step = progress.step.load(Ordering::Relaxed);
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_eq!(progress.step.load(Ordering::Relaxed), step);
    assert_eq!(progress.item.load(Ordering::Relaxed), 1);
    progress.abandon();
    assert!(progress.abandoned());
    release.send(()).unwrap();
    let (report, extrusions) = worker.join().unwrap();
    // Once released it did no more items nor definitions: the join after
    // it was never started, nor the final comparison, and no definition
    // was tried after the blocked one.
    assert_eq!(report.items.len(), 2, "{}", report.text());
    assert_eq!(extrusions, 1, "{}", report.text());
    assert_eq!(progress.item.load(Ordering::Relaxed), 1);
    assert!(report.bodies.is_empty(), "{}", report.text());
    assert!(progress.failed_items().is_empty());
}

#[test]
fn an_extrusion_out_of_memory_takes_its_state_as_do_the_items_while_memory_is_low() {
    // The kernel's first extrusion fails for lack of memory (a C++
    // allocation that failed, mitcad#80): the process is low on memory, and
    // with no memory guard to say it recovered, it stays so.
    let (items, mut geometry) = block_history(Vec::new());
    let kernel = TestKernel::default();
    kernel.out_of_memory.set(1);
    let mut doc = Document::new(kernel);
    let progress = Arc::new(crate::Progress::new());
    let options = Options {
        progress: Some(progress.clone()),
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
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(LOW_MEMORY), "{}", report.text());
    assert!(note(2).contains(LOW_MEMORY), "{}", report.text());
    // No extrusion was evaluated after the one that ran out of memory.
    assert_eq!(doc.kernel().mock.count("extrude_feature"), 0);
    assert_eq!(doc.bodies().len(), 1);
    let low = report.low_memory.clone().expect("the report says where");
    assert_eq!(low.item, Some(1));
    assert_eq!(low.name.as_deref(), Some("Extrude1"));
    assert_eq!(low.items, 2);
    assert!(
        low.memory.contains(KernelError::OUT_OF_MEMORY),
        "{}",
        low.memory
    );
    assert!(
        report.warnings.iter().any(|w| w.contains(
            "ran low on memory at Extrude1 (a geometry kernel operation ran out of memory"
        ) && w.ends_with("compared by volume only")),
        "{}",
        report.text()
    );
    // Not the items' own failures: a try run again tries them.
    assert!(progress.failed_items().is_empty());
}

#[test]
fn a_definition_cut_short_for_lack_of_memory_falls_back_and_the_import_goes_on() {
    // While the first extrusion is evaluated, the memory guard finds the
    // process low on memory (mitcad#80): the evaluation stops, as the
    // kernel's long operations do on request, and the item takes its
    // state. The memory has recovered for the join after it.
    let progress = Arc::new(crate::Progress::new());
    let guard = progress.clone();
    let worker = import_block_on_thread(
        Options {
            progress: Some(progress.clone()),
            ..Options::default()
        },
        Box::new(move |mock| {
            guard.low_memory("9.9 GiB of 10.0 GiB (test)");
            slow_call(mock);
            guard.memory_recovered();
        }),
    );
    let (report, extrusions) = worker.join().unwrap();
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(LOW_MEMORY), "{}", report.text());
    // The join was tried (the mock kernel's join on the fallback's body is
    // not its state).
    assert!(!note(2).contains(LOW_MEMORY), "{}", report.text());
    assert!(extrusions > 1, "{}", report.text());
    let low = report.low_memory.clone().expect("the report says where");
    assert_eq!((low.item, low.items), (Some(1), 1));
    assert_eq!(low.memory, "9.9 GiB of 10.0 GiB (test)");
    let warning = report
        .warnings
        .iter()
        .find(|w| w.contains("ran low on memory"))
        .expect("a warning");
    assert!(
        warning.ends_with("cut short or not tried and 1 modelling items took the file's bodies"),
        "{warning}"
    );
}

#[test]
fn memory_tight_drops_what_the_import_can_build_again() {
    // Asked to while the first extrusion is evaluated (mitcad#80), the
    // import drops before its next item the cached results of the
    // definitions it tried and the bodies of the history states behind it;
    // what it imports stays the same.
    let run = |tight: bool| {
        let progress = Arc::new(crate::Progress::new());
        let guard = progress.clone();
        let worker = import_block_on_thread(
            Options {
                progress: Some(progress.clone()),
                ..Options::default()
            },
            Box::new(move |_| {
                if tight {
                    guard.tight_on_memory();
                }
            }),
        );
        let (report, _) = worker.join().unwrap();
        (report, progress)
    };
    let (plain, _) = run(false);
    let (report, progress) = run(true);
    let outcomes = |r: &DesignReport| r.items.iter().map(|i| i.outcome).collect::<Vec<_>>();
    assert_eq!(outcomes(&report), outcomes(&plain), "{}", report.text());
    let json = |r: &DesignReport| serde_json::to_value(r).unwrap();
    assert_eq!(json(&report), json(&plain));
    assert!(report.low_memory.is_none());
    assert!(!progress.take_tight());
}

#[test]
fn the_oracle_builds_released_states_again() {
    let (_, mut geometry) = block_history(Vec::new());
    let kernel = TestKernel::default();
    let mut oracle = Oracle::new(&mut geometry);
    let before: Vec<Sig> = oracle.sigs(&kernel, 1);
    oracle.sigs(&kernel, 2);
    oracle.cursor = Some(2);
    assert_eq!(oracle.kept(), 2);
    assert_eq!(oracle.release_behind(), 1);
    assert_eq!(oracle.kept(), 1);
    assert_eq!(oracle.sigs(&kernel, 1), before);
    // (Built again, not counted again.)
    assert_eq!(oracle.built, 2);
}

#[test]
fn a_definition_that_takes_longer_than_its_item_is_cut_short() {
    // The first extrusion's kernel call would take 30 s, checking for a
    // request to stop as the geometry kernel's long operations do; the item
    // has a quarter of a second (mitcad#69). Its only definition has the
    // whole time (of several, the first would give way at its share).
    let started = std::time::Instant::now();
    let worker = import_block_on_thread(
        Options {
            item_seconds: 0.25,
            max_candidates: 1,
            ..Options::default()
        },
        Box::new(slow_call),
    );
    let (report, _) = worker.join().unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    assert!(
        elapsed < 10.0,
        "the item's time did not cut it short: {elapsed} s"
    );
    // The item gave up like one out of time and took its state; the join
    // after it was tried in its own time (the mock kernel's join on the
    // fallback's body is not the state).
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(
        outcomes,
        [Outcome::Partial, Outcome::Fallback, Outcome::Fallback],
        "{}",
        report.text()
    );
    let note = |i: usize| report.items[i].note.clone().unwrap_or_default();
    assert!(note(1).contains(OUT_OF_TIME), "{}", report.text());
    // It says which definition the time went to (mitcad#78).
    assert!(
        note(1).contains("; cut short after 0.")
            && note(1).ends_with(" s: definition 1 of 1 (extrude, new_body, 1 profile)"),
        "{}",
        report.text()
    );
    assert!(!note(2).contains(OUT_OF_TIME), "{}", report.text());
}

/// The hang watchdog of `mitcad-ffi` in short: watches the progress of the
/// import `worker` runs and gives the try up ([`Progress::abandon`]) once it
/// made no step for longer than `limit`. The worker's result, and the
/// longest time the import made no step.
fn watched<T>(
    progress: &Progress,
    limit: std::time::Duration,
    worker: std::thread::JoinHandle<T>,
) -> (T, std::time::Duration) {
    use std::sync::atomic::Ordering;
    let mut step = progress.step.load(Ordering::Relaxed);
    let mut moved = std::time::Instant::now();
    let mut longest = std::time::Duration::ZERO;
    while !worker.is_finished() {
        std::thread::sleep(std::time::Duration::from_millis(5));
        let now = progress.step.load(Ordering::Relaxed);
        if now != step {
            step = now;
            moved = std::time::Instant::now();
        }
        longest = longest.max(moved.elapsed());
        if moved.elapsed() > limit {
            progress.abandon();
        }
    }
    (worker.join().unwrap(), longest)
}

/// A design whose first item is a base feature of `bodies` stored bodies
/// (the file's first history state), imported on a thread of its own by a
/// test kernel whose `mass_properties` calls run `hook`.
fn import_base_feature_on_thread(
    bodies: usize,
    progress: Arc<Progress>,
    hook: Box<dyn FnMut(&MockKernel) + Send>,
) -> std::thread::JoinHandle<DesignReport> {
    std::thread::spawn(move || {
        let stored: Vec<MockShape> = (0..bodies)
            .map(|i| MockShape::imported(&format!("stored body {i}"), 6))
            .collect();
        let mut geometry = TestGeometry {
            states: vec![Vec::new(), stored],
            items: HashMap::from([(0, 1)]),
            ..TestGeometry::default()
        };
        let items = vec![json!({"index": 0, "name": "Base Feature1", "objectType": "BaseFeature"})];
        let kernel = TestKernel::default();
        *kernel.on_call.borrow_mut() = Some(("mass_properties", hook));
        let mut doc = Document::new(kernel);
        let options = Options {
            progress: Some(progress),
            ..Options::default()
        };
        import_design(&mut doc, &timeline_dump(items), &mut geometry, &options)
    })
}

#[test]
fn long_loops_of_the_import_tick_the_watchdog() {
    // Measuring the bodies of a large history state takes long, a kernel
    // call each (mitcad#82: 110 s on the first item of a large design,
    // whose hang limit was 60 s): here 30 bodies of 20 ms each, measured
    // as the state, as the replay's and as the stored design's, under a
    // hang limit of 0.3 s. The import ticks between the calls, so the
    // watchdog does not take it for a hang.
    let limit = std::time::Duration::from_millis(300);
    let progress = Arc::new(Progress::new());
    let started = std::time::Instant::now();
    let worker = import_base_feature_on_thread(
        30,
        progress.clone(),
        Box::new(|_| std::thread::sleep(std::time::Duration::from_millis(20))),
    );
    let (report, longest) = watched(&progress, limit, worker);
    let took = started.elapsed();
    assert!(took > 2 * limit, "the import took only {took:?}");
    assert!(
        !progress.abandoned(),
        "no step for {longest:?}: {}",
        report.text()
    );
    let outcomes: Vec<_> = report.items.iter().map(|i| i.outcome).collect();
    assert_eq!(outcomes, [Outcome::Fallback], "{}", report.text());
    assert_eq!(report.bodies.len(), 30, "{}", report.text());
}

#[test]
fn a_kernel_call_that_does_not_return_still_looks_hung() {
    // The same import, one measurement blocking for longer than the hang
    // limit (a kernel call that does not return): the ticks around it do
    // not hide it, and the watchdog gives the try up.
    let limit = std::time::Duration::from_millis(300);
    let progress = Arc::new(Progress::new());
    let mut calls = 0;
    let worker = import_base_feature_on_thread(
        30,
        progress.clone(),
        Box::new(move |_| {
            calls += 1;
            if calls == 5 {
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
        }),
    );
    let (report, longest) = watched(&progress, limit, worker);
    assert!(progress.abandoned(), "{}", report.text());
    assert!(longest > limit, "{longest:?}");
    // Given up: the stored design was not compared.
    assert!(report.bodies.is_empty(), "{}", report.text());
}

#[test]
fn definitions_that_gave_way_wait_for_the_rest_of_the_time() {
    let mut turns = Turns::default();
    // A share of the item's time each, the last one the rest.
    assert_eq!(turns.share(60.0, false, false), Some(SHARE * 60.0));
    assert_eq!(turns.share(60.0, true, false), None);
    turns.cut_short(1, 15.0, true);
    turns.cut_short(0, 16.0, true);
    // While some wait, the last one has a share too; they have the rest.
    assert_eq!(turns.share(60.0, true, false), Some(SHARE * 60.0));
    assert_eq!(turns.share(60.0, false, true), None);
    let join = json!({"type": "extrude", "operation": "join",
                      "profiles": [{"sketch": "F1", "region": "r{c1}"}]});
    let mut limited = join.clone();
    limited["participants"] = json!(["F2.b0"]);
    let candidates = vec![
        Candidate::new(json!({"type": "fillet"})),
        Candidate::new(join),
        Candidate::new(limited),
    ];
    // The join limited to participants waits behind the join untried.
    assert!(turns.waits_behind(&candidates, 2));
    assert!(!turns.waits_behind(&candidates, 0));
    // By rank, while more time is left than they ran.
    assert_eq!(turns.again(20.0), Some(1));
    assert_eq!(turns.again(16.0), Some(2));
    assert_eq!(turns.again(14.0), None);
    turns.cut_short(1, 19.0, false);
    let (seconds, note) = turns.note(&candidates).unwrap();
    assert_eq!(seconds, 19.0);
    assert_eq!(
        note,
        "cut short after 19.0 s: definition 2 of 3 (extrude, join, 1 profile) (and 1 other)"
    );
}

/// The large rectangle of `two_rectangles` (Mitcad's key; [`SMALL`] the
/// small one).
const LARGE: &str = "r{c9[c12,c10],c10[c9,c11],c11[c10,c12],c12[c11,c9]}";

/// One profile of the two rectangles extruded as a new body by 10 mm (the
/// decoded direction along the sketch's normal first), against the file's
/// state `right`: an extrusion of a region by a distance (mm), flipped or
/// not, which the history names when `known`; `slow` extrusions take
/// long (mitcad#78). The report, the definition of the item's feature
/// (when it has one), how many slow extrusions were asked for, and the
/// seconds the import took.
fn slow_profile_import(
    known: bool,
    right: (&str, f64, bool),
    slow: SlowExtrusions,
    item_seconds: f64,
) -> (DesignReport, Value, usize, f64) {
    let items = vec![
        two_rectangles(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let kernel = || TestKernel {
        directed: true,
        ..TestKernel::default()
    };
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items[..1].to_vec()),
        &mut NoGeometry,
        &Options::default(),
    );
    let (region, distance, flip) = right;
    add(
        &mut reference,
        json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
               "extent": {"type": "distance", "distance": distance}, "flip": flip,
               "operation": "new_body"}),
    );
    let mut geometry = TestGeometry {
        states: vec![Vec::new(), shapes(&reference)],
        items: if known {
            HashMap::from([(1, 1)])
        } else {
            HashMap::new()
        },
        ..TestGeometry::default()
    };
    let mut doc = Document::new(TestKernel {
        slow: Some(slow),
        ..kernel()
    });
    let started = std::time::Instant::now();
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut geometry,
        &Options {
            item_seconds,
            ..Options::default()
        },
    );
    let elapsed = started.elapsed().as_secs_f64();
    let def = report.items[1]
        .features
        .first()
        .and_then(|uid| {
            doc.query(&format!(r#"{{"query": "feature", "uid": "{uid}"}}"#))
                .ok()
        })
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .map(|def| def["def"].clone())
        .unwrap_or_default();
    (report, def, doc.kernel().slow_calls.get(), elapsed)
}

#[test]
fn a_slow_definition_gives_way_to_the_next_ones() {
    // The file's extrusion goes against the sketch's normal; the one
    // along it is tried first and would take 30 s: at its share of the
    // item's 2 s it gives way, and the next one gives the state (mitcad#78).
    // Before, it used up the item's time.
    let slow = SlowExtrusions {
        regions: &[LARGE],
        along: true,
        seconds: 30.0,
    };
    for known in [true, false] {
        let (report, def, slow, elapsed) =
            slow_profile_import(known, (LARGE, 10.0, true), slow, 2.0);
        assert_eq!(
            report.items[1].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        assert_eq!(report.items[1].verified, Some(true));
        assert_eq!(def["profiles"][0]["region"], LARGE, "{}", report.text());
        assert_eq!(def["flip"], true, "{}", report.text());
        assert_eq!(slow, 1, "{}", report.text());
        assert!(elapsed < 10.0, "{elapsed} s");
    }
}

#[test]
fn a_definition_that_gave_way_gets_the_rest_of_the_time() {
    // The file's extrusion, tried first, takes 1 s, more than its share (a
    // third of 2 s): it gives way, the others give no state, and it is
    // tried again with the rest of the item's time and gives the state.
    let slow = SlowExtrusions {
        regions: &[LARGE],
        along: true,
        seconds: 1.0,
    };
    for known in [true, false] {
        let (report, def, slow, _) = slow_profile_import(known, (LARGE, 10.0, false), slow, 2.0);
        assert_eq!(
            report.items[1].outcome,
            Outcome::Parametric,
            "{}",
            report.text()
        );
        assert_eq!(def["profiles"][0]["region"], LARGE, "{}", report.text());
        assert_ne!(def["flip"], true, "{}", report.text());
        assert_eq!(slow, 2, "{}", report.text());
    }
}

#[test]
fn an_item_out_of_time_names_the_definition_its_time_went_to() {
    // No definition gives the file's state (an extrusion by 20 mm), and
    // the one of both rectangles along the sketch's normal takes longer
    // than the item's second: it gives way, is tried again with the rest
    // of the time and cut short there; the item falls back and says which
    // definition its time went to.
    let slow = SlowExtrusions {
        regions: &[LARGE, SMALL],
        along: true,
        seconds: 30.0,
    };
    for known in [true, false] {
        let (report, _, slow, elapsed) =
            slow_profile_import(known, (SMALL, 20.0, false), slow, 1.0);
        assert_eq!(
            report.items[1].outcome,
            Outcome::Fallback,
            "{}",
            report.text()
        );
        let note = report.items[1].note.clone().unwrap_or_default();
        assert!(
            note.contains(&format!("{OUT_OF_TIME}; cut short after 0."))
                && note.contains(" (extrude, new_body, 2 profiles)"),
            "{}",
            report.text()
        );
        assert!(slow >= 1, "{}", report.text());
        assert!(elapsed < 10.0, "{elapsed} s");
    }
}

#[test]
fn points_along_a_segment_are_inside_between_its_crossings() {
    use crate::ops::inside_at;
    // In at a quarter, out at a half, in again at three quarters.
    let crossings = [(0.25, true), (0.5, false), (0.75, true)];
    let inside: Vec<bool> = [0.1, 0.3, 0.6, 0.9]
        .iter()
        .map(|t| inside_at(&crossings, *t))
        .collect();
    assert_eq!(inside, [false, true, false, true]);
    // Out at a half only: inside before it.
    assert!(inside_at(&[(0.5, false)], 0.2));
    assert!(!inside_at(&[(0.5, false)], 0.7));
    // No crossing: outside.
    assert!(!inside_at(&[], 0.5));
}

#[test]
fn a_sizing_offset_that_takes_long_gives_way_to_the_next_definition() {
    // The offset of the threaded face would take 30 s: it is cut short at
    // its few seconds and the thread comes in without it (mitcad#68).
    let kernel = || TestKernel {
        face_listing: true,
        ..TestKernel::default()
    };
    let mut items = vec![
        circle_sketch(0),
        extrude(1, 0, "NewBodyFeatureOperation", "d2"),
    ];
    let mut reference = Document::new(kernel());
    import_design(
        &mut reference,
        &timeline_dump(items.clone()),
        &mut NoGeometry,
        &Options::default(),
    );
    let side = reference.bodies()[0]
        .shape
        .faces
        .iter()
        .find(|f| f.role == "side")
        .cloned()
        .expect("a side face");
    let info = json!({"_type": "ThreadInfo", "threadType": "ISO Metric profile",
        "threadDesignation": "M10x1.5", "threadClass": "6g", "isInternal": false,
        "majorDiameter": 0.985, "minorDiameter": 0.8141});
    items.push(thread_item(
        2,
        info,
        true,
        1.0,
        face_point(&side).map(|x| x / 10.0),
    ));
    let k = kernel();
    *k.on_call.borrow_mut() = Some(("offset_faces", Box::new(slow_call)));
    let mut doc = Document::new(k);
    let started = std::time::Instant::now();
    let report = import_design(
        &mut doc,
        &timeline_dump(items),
        &mut NoGeometry,
        &Options::default(),
    );
    let elapsed = started.elapsed().as_secs_f64();
    assert!(
        (SIZING_OFFSET_SECONDS..10.0).contains(&elapsed),
        "{elapsed} s"
    );
    assert_eq!(
        report.items[2].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
    assert!(
        !doc.features()
            .any(|f| matches!(f.def, FeatureDef::OffsetFace(_))),
        "{}",
        report.text()
    );
}

// Joints, as-built joints, joint origins and ground items (mitcad#55).
mod joint_tests;
