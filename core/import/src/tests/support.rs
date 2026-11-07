// SPDX-License-Identifier: MIT
//! Shared by the topic modules: the test kernel, the stored-geometry fixture and the builders of the block design and of sketches.

use super::*;
use mitcad_model_api::DocumentApi;

pub(super) fn dump(value: Value) -> Dump {
    Dump::from_json(&value.to_string()).expect("a dump")
}

pub(super) fn param(name: &str, expression: &str, value: f64, unit: &str) -> Value {
    json!({"kind": "parameter", "name": name, "expression": expression, "value": value, "unit": unit})
}

/// A 40 x 20 mm rectangle on XY from (0, 0) (cm in the IR), its width
/// dimensioned with d1.
pub(super) fn rectangle_sketch(index: i64) -> Value {
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

pub(super) fn extrude(index: i64, sketch: i64, operation: &str, distance: &str) -> Value {
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

pub(super) fn block_dump() -> Dump {
    dump(json!({
        "schema": "mitcad-f3d-dump", "schema_version": 2,
        "source": {"mode": "f3d_stream", "file": "block.f3d"},
        "parameters": {
            "user": [{"name": "width", "expression": "height * 2", "value": 4.0, "unit": "mm"},
                     {"name": "height", "expression": "20 mm", "value": 2.0, "unit": "mm",
                      "isFavorite": true}],
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

pub(super) fn import(doc: &mut Document<MockKernel>, dump: &Dump) -> DesignReport {
    import_design(doc, dump, &mut NoGeometry, &Options::default())
}

/// The mock kernel with mass properties: a solid's "volume" and "area"
/// come from its history and face names with feature numbers left out,
/// so the same operations give the same values in another document, and
/// a body read back from B-rep data keeps the values of the one written.
#[derive(Default)]
pub(super) struct TestKernel {
    pub(super) mock: MockKernel,
    /// Mock B-rep history → the written shape's fingerprint.
    pub(super) written: std::cell::RefCell<std::collections::HashMap<String, String>>,
    /// Whether edges have middles (see edge_point), so that the edge
    /// fingerprints of external dumps resolve.
    pub(super) edge_points: bool,
    /// Whether faces are listed with a point each (see face_point), so
    /// that face fingerprints resolve.
    pub(super) face_listing: bool,
    /// Whether the solids this kernel's extrusions make come out 3e-4
    /// larger than the same extrusion of another kernel (a marker in their
    /// history), as an extrusion of regions next to the file's would.
    pub(super) skew: bool,
    /// Whether solids without copies of a pattern all measure the same, so
    /// that only patterns tell extrusions of different regions apart.
    pub(super) alike: bool,
    /// Whether the kernel lists the faces a body has and others lack
    /// (`Kernel::new_faces`): by their names without feature numbers, a
    /// point each (see face_point), so that a skewed body lies on the
    /// stored one (mitcad#283).
    pub(super) lists_new_faces: bool,
    /// Whether the solids this kernel's fillets make come out 3e-4 larger
    /// (as `skew` for extrusions): an approximation of the file's own kind
    /// that the bodies after them carry (mitcad#283).
    pub(super) skew_fillets: bool,
    /// Whether this kernel's fillets carry a marker (`≈` in their history)
    /// that makes them measure `fillet_noise` mm³ more than they bound, as
    /// a stored rounding whose edges lie off its faces measures
    /// (mitcad#235).
    pub(super) noisy_fillets: bool,
    /// mm³ this kernel's fillets take from their body, where given (else
    /// their volume comes from their history as other solids').
    pub(super) fillet_takes: Option<f64>,
    pub(super) fillet_noise: f64,
    /// mm³ this kernel's fillets (with `fillet_takes`) leave on their body
    /// beyond what they should take, marked `∆` in their history: a result
    /// that deviates from the file's in its volume only (mitcad#379).
    pub(super) fillet_off: f64,
    /// The slack `Kernel::changed_volume` gives, where it measures at all:
    /// the change as the bodies' volumes, noise left out.
    pub(super) change_slack: Option<f64>,
    /// Runs in each kernel call of this name ("extrude_feature",
    /// "offset_faces", "mass_properties", "transform_shape") before it is
    /// made: a call that
    /// blocks or takes long (mitcad#68, mitcad#69, mitcad#71, mitcad#82).
    #[allow(clippy::type_complexity)]
    pub(super) on_call:
        std::cell::RefCell<Option<(&'static str, Box<dyn FnMut(&MockKernel) + Send>)>>,
    /// How many of the next extrusions fail as an operation whose
    /// allocation failed does (mitcad#80).
    pub(super) out_of_memory: std::cell::Cell<usize>,
    /// Extrusions that take long, every time (mitcad#78).
    pub(super) slow: Option<SlowExtrusions>,
    /// How many of them were asked for.
    pub(super) slow_calls: std::cell::Cell<usize>,
    /// Whether extrusions against the sketch's normal differ from those
    /// along it (a marker in their history; the mock kernel's are alike).
    pub(super) directed: bool,
    /// Whether a shape moved without copying (`transform_shape` without an
    /// instance) measures as the shape it was, its centre moved by the
    /// translation, as a rigid motion keeps a solid's measures (mitcad#465).
    pub(super) rigid: bool,
    /// Shapes whose history contains this are invalid for the kernel's
    /// checker (`Kernel::is_valid`).
    pub(super) invalid: Option<&'static str>,
    /// How many slow extrusions began, and how many were cut short by a
    /// request to stop, in this kernel and its forks (mitcad#95).
    pub(super) began: Arc<std::sync::atomic::AtomicUsize>,
    pub(super) stopped: Arc<std::sync::atomic::AtomicUsize>,
    /// How many times points were classified (probing regions against the
    /// history, mitcad#181); the kernel does not classify them.
    pub(super) probes: Arc<std::sync::atomic::AtomicUsize>,
}

/// Extrusions of `regions` (along the sketch's normal only, with `along`)
/// take `seconds`, checking for a request to stop as `slow_call`.
#[derive(Clone, Copy)]
pub(super) struct SlowExtrusions {
    pub(super) regions: &'static [&'static str],
    pub(super) along: bool,
    /// Against the sketch's normal only (mitcad#95).
    pub(super) against: bool,
    pub(super) seconds: f64,
    /// Not checking for a request to stop: a kernel call that runs on past
    /// the end of its time (mitcad#434).
    pub(super) deaf: bool,
}

impl TestKernel {
    pub(super) fn hook(&self, call: &str) {
        if let Some((name, hook)) = self.on_call.borrow_mut().as_mut()
            && *name == call
        {
            hook(&self.mock);
        }
    }

    /// The volume of a solid of the fingerprint `f` (see `mass_properties`):
    /// the last `[=v]` in it (`fillet_takes`), else its hash; `~` scales it
    /// by 1.0003, `≈` adds `fillet_noise`.
    pub(super) fn volume_of(&self, f: &str) -> f64 {
        let scale = if f.contains('~') { 1.0003 } else { 1.0 };
        let noise = if f.contains('≈') {
            self.fillet_noise
        } else {
            0.0
        };
        let given = f.rfind("[=").and_then(|at| {
            let rest = &f[at + 2..];
            rest[..rest.find(']')?].parse::<f64>().ok()
        });
        let off = if f.contains('∆') {
            self.fillet_off
        } else {
            0.0
        };
        given.unwrap_or_else(|| hash(&f.replace(['~', '≈', '∆'], ""), 1)) * scale + noise + off
    }
}

/// A kernel call that takes up to 30 s, checking for a request to stop as
/// the geometry kernel's long operations do (`interruptible`).
pub(super) fn slow_call(mock: &MockKernel) {
    slow_call_for(mock, 30.0);
}

/// A kernel call that takes up to `seconds`, as [`slow_call`].
pub(super) fn slow_call_for(mock: &MockKernel, seconds: f64) -> bool {
    mock.stop_on_cancel.set(true);
    let begun = std::time::Instant::now();
    while begun.elapsed().as_secs_f64() < seconds {
        if mock
            .interrupt
            .borrow()
            .as_ref()
            .is_some_and(|m| m.is_cancelled())
        {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    false
}

/// A kernel call that works on the processor for up to `seconds` (its
/// thread's own time, as a long offset does; [`slow_call_for`] waits
/// instead), stopping early on a cancel as the kernel's long operations
/// do: whether it was cancelled.
pub(super) fn busy_call_for(mock: &MockKernel, seconds: f64) -> bool {
    mock.stop_on_cancel.set(true);
    let begun = std::time::Instant::now();
    let mut spin = 0u64;
    while begun.elapsed().as_secs_f64() < seconds {
        if mock
            .interrupt
            .borrow()
            .as_ref()
            .is_some_and(|m| m.is_cancelled())
        {
            return true;
        }
        for i in 0..100_000u64 {
            spin = std::hint::black_box(spin.wrapping_mul(31).wrapping_add(i));
        }
    }
    false
}

/// The import's clock at the wall clock's pace for the whole test process
/// (mitcad#874): a test whose outcome hangs on a time limit (the item's,
/// a share, the design's, a definition's own) calls it first, so that the
/// limit runs out when the test expects however loaded the machine is.
pub(super) fn wall_clock() {
    mitcad_model::monitor::wall_pace_for_tests();
}

/// Feature numbers left out: `F12` becomes `F`.
pub(super) fn without_uids(text: &str) -> String {
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
    pub(super) fn fingerprint(&self, shape: &MockShape) -> String {
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

pub(super) fn hash(text: &str, salt: u64) -> f64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (text, salt).hash(&mut h);
    100.0 + (h.finish() % 100_000) as f64 / 10.0
}

impl Kernel for TestKernel {
    type Shape = MockShape;

    fn is_valid(&self, shape: &MockShape) -> Result<bool, KernelError> {
        match self.invalid {
            Some(marker) => Ok(!shape.history.contains(marker)),
            None => Err(KernelError::Unsupported("validity checks")),
        }
    }

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
            && (!slow.against || spec.direction[2] < 0.0)
        {
            self.slow_calls.set(self.slow_calls.get() + 1);
            self.began.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            if slow.deaf {
                std::thread::sleep(std::time::Duration::from_secs_f64(slow.seconds));
            } else if slow_call_for(&self.mock, slow.seconds) {
                self.stopped
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
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
        let mut shape = self.mock.fillet(feature, body, sets, rolling)?;
        if self.skew_fillets {
            shape.history.push('~');
        }
        if let Some(takes) = self.fillet_takes {
            // (A body that deviates already: its `∆` counts once, below.)
            let body_print = self.fingerprint(body);
            let off = if body_print.contains('∆') {
                self.fillet_off
            } else {
                0.0
            };
            let base = self.volume_of(&body_print) - off;
            shape.history.push_str(&format!("[={}]", base - takes));
            if self.fillet_off != 0.0 && !shape.history.contains('∆') {
                shape.history.push('∆');
            }
        }
        if self.noisy_fillets {
            shape.history.push('≈');
        }
        Ok(shape)
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
        self.hook("transform_shape");
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
        // A moved shape (`moved(<history>,x,y,z)`): its original's measures.
        if self.rigid
            && let Some(inner) = shape
                .history
                .strip_prefix("moved(")
                .and_then(|h| h.strip_suffix(')'))
        {
            let mut parts = inner.rsplitn(4, ',');
            let offset: Vec<f64> = parts
                .by_ref()
                .take(3)
                .filter_map(|p| p.parse().ok())
                .collect();
            if let (Some(history), [z, y, x]) = (parts.next(), offset.as_slice()) {
                let original = MockShape {
                    history: history.to_owned(),
                    ..shape.clone()
                };
                let mut m = self.mass_properties(&original)?;
                m.center = [m.center[0] + x, m.center[1] + y, m.center[2] + z];
                return Ok(m);
            }
        }
        let f = self.fingerprint(shape);
        if self.alike && !f.contains("moved(") {
            // (Less than a prism of either rectangle adds.)
            return Ok(MassProperties {
                volume: 1000.0,
                area: 1000.0,
                center: [0.0; 3],
            });
        }
        Ok(MassProperties {
            volume: self.volume_of(&f),
            area: hash(&f.replace(['~', '≈', '∆'], ""), 2),
            center: [0.0; 3],
        })
    }

    // The bodies' volumes, where `change_slack` is set.
    fn changed_volume(
        &self,
        before: &[&MockShape],
        after: &[&MockShape],
    ) -> Result<(f64, f64), KernelError> {
        let slack = self
            .change_slack
            .ok_or(KernelError::Unsupported("changed volume"))?;
        let total = |shapes: &[&MockShape]| -> f64 {
            shapes
                .iter()
                .map(|s| self.volume_of(&self.fingerprint(s)))
                .sum()
        };
        Ok((total(after) - total(before), slack))
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

    fn shell(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        spec: &mitcad_model::ShellSpec<'_>,
    ) -> Result<MockShape, KernelError> {
        self.mock.shell(feature, body, spec)
    }

    fn loft(&self, spec: &mitcad_model::LoftSpec<'_, MockShape>) -> Result<MockShape, KernelError> {
        self.mock.loft(spec)
    }

    // Sweeps, plain and along guide surfaces (mitcad#157).
    fn sweep(&self, spec: &mitcad_model::SweepSpec<'_>) -> Result<MockShape, KernelError> {
        self.mock.sweep(spec)
    }

    // Primitives and coils (mitcad#25).
    fn primitive(&self, spec: &mitcad_model::PrimitiveSpec) -> Result<MockShape, KernelError> {
        self.mock.primitive(spec)
    }

    fn coil(&self, spec: &mitcad_model::CoilSpec) -> Result<MockShape, KernelError> {
        self.mock.coil(spec)
    }

    fn surface_sweep(
        &self,
        spec: &mitcad_model::SweepSpec<'_>,
        surfaces: &[mitcad_model::GuideSurface<'_, MockShape>],
    ) -> Result<MockShape, KernelError> {
        self.mock.surface_sweep(spec, surfaces)
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

    // The edges the test gives curves (`mock.edge_curves`); the others
    // are not measured.
    fn edge_geometry(
        &self,
        shape: &MockShape,
        edge: &EdgeName,
    ) -> Result<mitcad_model::datum::CurveGeometry, KernelError> {
        if !self
            .mock
            .edge_curves
            .borrow()
            .contains_key(&edge.to_string())
        {
            return Err(KernelError::Unsupported("edge geometry"));
        }
        self.mock.edge_geometry(shape, edge)
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

    fn move_faces(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        faces: &[mitcad_model::FaceName],
        motion: &mitcad_model::Transform,
    ) -> Result<MockShape, KernelError> {
        self.mock.move_faces(feature, body, faces, motion)
    }

    fn draft(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        spec: &mitcad_model::DraftSpec<'_, MockShape>,
    ) -> Result<MockShape, KernelError> {
        self.mock.draft(feature, body, spec)
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

    fn new_faces(
        &self,
        before: &[&MockShape],
        after: &MockShape,
        _count: usize,
    ) -> Result<Vec<mitcad_model::NewFace>, KernelError> {
        if !self.lists_new_faces {
            return Err(KernelError::Unsupported("new faces"));
        }
        let had: BTreeSet<String> = before
            .iter()
            .flat_map(|s| s.faces.iter().map(|f| without_uids(&f.to_string())))
            .collect();
        Ok(after
            .faces
            .iter()
            .filter(|f| !had.contains(&without_uids(&f.to_string())))
            .map(|f| mitcad_model::NewFace {
                area: 1.0,
                points: vec![face_point(f)],
            })
            .collect())
    }

    // Definitions evaluated ahead (mitcad#95): the same settings; not with
    // the hooks of a test that counts or blocks calls.
    fn fork(&self) -> Option<Self> {
        if self.on_call.borrow().is_some() || self.out_of_memory.get() > 0 {
            return None;
        }
        Some(TestKernel {
            mock: self.mock.fork()?,
            written: self.written.clone(),
            edge_points: self.edge_points,
            face_listing: self.face_listing,
            skew: self.skew,
            alike: self.alike,
            lists_new_faces: self.lists_new_faces,
            skew_fillets: self.skew_fillets,
            noisy_fillets: self.noisy_fillets,
            fillet_takes: self.fillet_takes,
            fillet_noise: self.fillet_noise,
            fillet_off: self.fillet_off,
            change_slack: self.change_slack,
            on_call: Default::default(),
            out_of_memory: Default::default(),
            slow: self.slow,
            slow_calls: Default::default(),
            directed: self.directed,
            rigid: self.rigid,
            invalid: self.invalid,
            began: Arc::clone(&self.began),
            stopped: Arc::clone(&self.stopped),
            probes: Arc::clone(&self.probes),
        })
    }

    fn points_inside(
        &self,
        _shape: &MockShape,
        _points: &[[f64; 3]],
    ) -> Result<Vec<bool>, KernelError> {
        self.probes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(KernelError::Unsupported("point classification"))
    }

    // Surface modelling (mitcad#455).
    fn open_faces(
        &self,
        shape: &MockShape,
        feature: FeatureUid,
        roles: &[&str],
    ) -> Result<MockShape, KernelError> {
        self.mock.open_faces(shape, feature, roles)
    }
}

/// The test kernel's point inside a face (the same in every document).
pub(super) fn face_point(face: &mitcad_model::FaceName) -> [f64; 3] {
    let name = without_uids(&face.to_string());
    [hash(&name, 3), hash(&name, 4), hash(&name, 5)]
}

/// The TestKernel's middle of the edge between two faces (mm).
pub(super) fn edge_point(a: &mitcad_model::FaceName, b: &mitcad_model::FaceName) -> [f64; 3] {
    let (p, q) = (face_point(a), face_point(b));
    [
        0.5 * (p[0] + q[0]),
        0.5 * (p[1] + q[1]),
        0.5 * (p[2] + q[2]),
    ]
}

/// History states and stored bodies given by the test.
#[derive(Default)]
pub(super) struct TestGeometry {
    pub(super) states: Vec<Vec<MockShape>>,
    /// The state each item made, by item index.
    pub(super) items: HashMap<i64, usize>,
    /// States that cannot be built.
    pub(super) broken: Vec<usize>,
    /// The component (object id) of every body and of the items' states.
    pub(super) component: Option<u64>,
    /// Bodies have ids: the same shape in different states the same.
    pub(super) ids: bool,
    /// Items whose state the history does not hold (suppressed).
    pub(super) without_result: Vec<i64>,
    /// Items whose operation changed no body (failed in the file).
    pub(super) unchanged: Vec<i64>,
    /// Items naming an earlier item's operation (failed in the file).
    pub(super) shared: Vec<i64>,
    /// The state before the operations no item names right before an
    /// item's own, by item index.
    pub(super) starts: HashMap<i64, usize>,
    /// Per state, the lineage (`StoredBody::lineage`) of each body.
    pub(super) lineages: Vec<Vec<u64>>,
    /// The states `release_before` was given, in order.
    pub(super) released_before: Vec<usize>,
}

impl StoredGeometry<MockShape> for TestGeometry {
    fn state_count(&mut self) -> usize {
        self.states.len()
    }

    fn item_state(&mut self, index: i64) -> Option<usize> {
        self.items.get(&index).copied()
    }

    fn item_without_result(&mut self, index: i64) -> bool {
        self.without_result.contains(&index)
    }

    fn item_unchanged(&mut self, index: i64) -> bool {
        self.unchanged.contains(&index)
    }

    fn item_start(&mut self, index: i64) -> Option<usize> {
        self.starts.get(&index).copied()
    }

    fn item_shares_operation(&mut self, index: i64) -> bool {
        self.shared.contains(&index)
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
                id: self
                    .ids
                    .then(|| self.states.iter().flatten().position(|s| s == shape))
                    .flatten()
                    .map(|p| p as u64),
                lineage: self.lineages.get(index).and_then(|l| l.get(i)).copied(),
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

    fn release_before(&mut self, state: usize) {
        self.released_before.push(state);
    }
}

pub(super) fn shapes(doc: &Document<TestKernel>) -> Vec<MockShape> {
    doc.bodies().iter().map(|b| b.shape.clone()).collect()
}

/// Two rectangles side by side: 40 x 20 and 10 x 20 mm.
pub(super) fn two_rectangles(index: i64) -> Value {
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

pub(super) fn timeline_dump(items: Vec<Value>) -> Dump {
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

pub(super) fn add(doc: &mut Document<TestKernel>, def: Value) {
    let def: FeatureDef<mitcad_model::ValueInput> = serde_json::from_value(def).unwrap();
    let added = doc.add_feature(&def, None).unwrap();
    assert_eq!(doc.status(added.uid).and_then(|s| s.error()), None);
}

/// The two rectangles with their curves' geometry and an extrusion whose
/// decoded profile loops select the small one (mitcad#96).
pub(super) fn decoded_small_rectangle() -> Vec<Value> {
    let mut sketch = two_rectangles(0);
    let corners = [
        ([0.0, 0.0], [4.0, 0.0]),
        ([4.0, 0.0], [4.0, 2.0]),
        ([4.0, 2.0], [0.0, 2.0]),
        ([0.0, 2.0], [0.0, 0.0]),
        ([5.0, 0.0], [6.0, 0.0]),
        ([6.0, 0.0], [6.0, 2.0]),
        ([6.0, 2.0], [5.0, 2.0]),
        ([5.0, 2.0], [5.0, 0.0]),
    ];
    for (c, (a, b)) in sketch["detail"]["curves"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .zip(corners)
    {
        c["geometry"] = json!({"type": "Line3D", "startPoint": [a[0], a[1], 0.0],
                               "endPoint": [b[0], b[1], 0.0]});
    }
    let record = |id: &str, k: u64| {
        json!({"id": id, "primary": k, "secondary": 0, "tag": 3, "reversed": false,
               "piece": 1, "pieces": 1})
    };
    let mut item = extrude(1, 0, "NewBodyFeatureOperation", "d2");
    item["detail"]["_f3d_profile_loops"] = json!([{"loops": [{"outer": true, "curves": [
        record("c4", 4), record("c5", 5), record("c6", 6), record("c7", 7)]}], "children": []}]);
    vec![sketch, item]
}

/// A fillet whose edges the decoder does not give, and an extrusion after
/// it.
pub(super) fn fillet_item(index: i64) -> Value {
    json!({"index": index, "name": "Fillet1", "objectType": "FilletFeature",
           "detail": {"edgeSets": [{"radius": param("d5", "2 mm", 0.2, "mm")}]}})
}

/// The block's history: nothing, the block, the block joined again.
pub(super) fn block_history(broken: Vec<usize>) -> (Vec<Value>, TestGeometry) {
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
        ids: false,
        ..TestGeometry::default()
    };
    (items, geometry)
}

/// A circle of 10 mm diameter on XY (cm in the IR).
pub(super) fn circle_sketch(index: i64) -> Value {
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
pub(super) fn thread_item(
    index: i64,
    info: Value,
    full: bool,
    axis_z: f64,
    point: [f64; 3],
) -> Value {
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

/// The block dump with components: the root (object 3), Plate (10) placed
/// twice 10 cm apart, each with Pin (20) 1 cm along X in it, and an
/// occurrence of a component of another document.
pub(super) fn assembly_dump(named: bool) -> Dump {
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

/// The smaller of the two rectangles' regions in Sketch1 (F1), 10 x 20 mm.
pub(super) const SMALL: &str = "r{c13[c16,c14],c14[c13,c15],c15[c14,c16],c16[c15,c13]}";

/// A document with Sketch1 of the two rectangles (F1) and the features
/// `defs` after it, made by the plain test kernel: the file's bodies.
pub(super) fn reference(defs: Vec<Value>) -> Document<TestKernel> {
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

pub(super) fn extrusion_of(region: &str) -> Value {
    json!({"type": "extrude", "profiles": [{"sketch": "F1", "region": region}],
           "extent": {"type": "distance", "distance": 10}, "operation": "new_body"})
}

// Polygons, sketch patterns, offsets and concentric circle dimensions
// (mitcad#36).

/// A dump of one sketch on XY with the given model parameters.
pub(super) fn group_sketch_dump(
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

pub(super) fn gpoint(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "xyz": [x, y, 0.0]})
}

pub(super) fn gfixed(id: &str, x: f64, y: f64) -> Value {
    json!({"id": id, "xyz": [x, y, 0.0], "isFixed": true})
}

pub(super) fn gline(id: &str, a: &str, b: &str) -> Value {
    json!({"id": id, "type": "SketchLine", "startSketchPoint": a, "endSketchPoint": b})
}

pub(super) fn gcircle(id: &str, center: &str, radius: f64) -> Value {
    json!({"id": id, "type": "SketchCircle", "centerSketchPoint": center, "radius": radius})
}

pub(super) fn sketch_query(doc: &Document<MockKernel>) -> Value {
    serde_json::from_str(&doc.query(r#"{"query": "sketch", "uid": "F1"}"#).unwrap()).unwrap()
}

/// The id of the point at `at` (sketch mm).
pub(super) fn point_at(s: &Value, at: [f64; 2]) -> String {
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

pub(super) fn entity<'a>(s: &'a Value, id: &str) -> &'a Value {
    s["entities"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id}"))
}

pub(super) fn at(s: &Value, id: &str) -> [f64; 2] {
    let e = entity(s, id);
    [e["at"][0].as_f64().unwrap(), e["at"][1].as_f64().unwrap()]
}

pub(super) fn radius(s: &Value, id: &str) -> f64 {
    entity(s, id)["geometry"]["radius"].as_f64().unwrap()
}

pub(super) fn assert_parametric(report: &DesignReport) {
    assert_eq!(
        report.items[0].outcome,
        Outcome::Parametric,
        "{}",
        report.text()
    );
}

/// The large rectangle of `two_rectangles` (Mitcad's key; [`SMALL`] the
/// small one).
pub(super) const LARGE: &str = "r{c9[c12,c10],c10[c9,c11],c11[c10,c12],c12[c11,c9]}";

/// One profile of the two rectangles extruded as a new body by 10 mm (the
/// decoded direction along the sketch's normal first), against the file's
/// state `right`: an extrusion of a region by a distance (mm), flipped or
/// not, which the history names when `known`; `slow` extrusions take
/// long (mitcad#78). The report, the definition of the item's feature
/// (when it has one), how many slow extrusions were asked for, and the
/// seconds the import took.
pub(super) fn slow_profile_import(
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
