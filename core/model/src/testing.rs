// SPDX-License-Identifier: MIT
//! A mock geometry kernel for the model tests. Shapes record the operations
//! that built them and carry face names and face-pair edges the way the
//! OCCT facade names them, so references, recompute and the body state can
//! be tested without OCCT. Tests steer booleans and failures through the
//! kernel's settings.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::datum::{CurveGeometry, SurfaceGeometry};
use crate::exchange::{BodyKind, ExportBody, ExportFormat, ExportOptions, ImportBody, ReadOptions};
use crate::ids::FeatureUid;
use crate::kernel::{
    Axis, Cylinder, ExtrudeFeatureSpec, ExtrudeSide, ExtrudeStart, HoleEnd, HoleSpec, Plane,
    RevolveSpec, SideEnd, Target, ThreadSpec, simple_extrude,
};
use crate::kernel::{
    BooleanOp, BooleanOutput, BooleanPiece, BoundingBox, Chamfer, ChamferCorner, ExtrudeSpec,
    Kernel, KernelError, MassProperties,
};
use crate::kernel::{DraftSpec, FilletSet, FilletSize, ShellSpec, ToolInput};
use crate::kernel::{FontGlyphs, FontRequest, Glyph};
use crate::monitor::RecomputeMonitor;
use crate::sweeps::{CoilSpec, LoftSection, LoftSpec, PipeSection, PipeSpec, RibSpec, SweepSpec};
use crate::sweeps::{LoftEndKind, PathCurve};
use crate::topo::{EdgeName, FaceName, RoleKey, SegmentKey};
use crate::transform::{Instance, PrimitiveShape, PrimitiveSpec, Transform};

#[derive(Debug, Clone, PartialEq)]
pub struct MockShape {
    /// The operations that built the shape, e.g. `cut(prism(F2:0..20),prism(F4:0..30))`.
    pub history: String,
    pub faces: Vec<FaceName>,
    /// Pairs of faces that meet in an edge.
    pub edges: Vec<(FaceName, FaceName)>,
    pub bounds: Option<BoundingBox>,
    /// The solids of a shape made of several, else empty.
    pub parts: Vec<MockShape>,
}

impl MockShape {
    fn has_face(&self, reference: &FaceName) -> bool {
        self.faces.iter().any(|f| f.matches(reference))
    }

    fn matching_edges(&self, edge: &EdgeName) -> Vec<&(FaceName, FaceName)> {
        let [a, b] = edge.faces();
        self.edges
            .iter()
            .filter(|(x, y)| (x.matches(a) && y.matches(b)) || (x.matches(b) && y.matches(a)))
            .collect()
    }

    fn merged(history: String, shapes: &[&MockShape], lost: &[FaceName]) -> MockShape {
        let mut faces = Vec::new();
        let mut edges = Vec::new();
        let mut bounds: Option<BoundingBox> = None;
        for shape in shapes {
            faces.extend(shape.faces.iter().filter(|f| !lost.contains(f)).cloned());
            edges.extend(
                shape
                    .edges
                    .iter()
                    .filter(|(a, b)| !lost.contains(a) && !lost.contains(b))
                    .cloned(),
            );
            bounds = match (bounds, shape.bounds) {
                (Some(a), Some(b)) => Some(a.union(&b)),
                (a, b) => a.or(b),
            };
        }
        MockShape {
            history,
            faces,
            edges,
            bounds,
            parts: Vec::new(),
        }
    }
}

#[derive(Default)]
pub struct MockKernel {
    /// Measures shapes (`mass_properties` of their bounding boxes; scaled
    /// patterns, mitcad#59); otherwise it measures nothing.
    pub measures: Cell<bool>,
    /// Operation name -> number of calls.
    pub calls: RefCell<BTreeMap<&'static str, usize>>,
    /// Operations that fail: "extrude", "join", "cut", "intersect", "fillet", "chamfer".
    pub fail: RefCell<BTreeSet<&'static str>>,
    /// Operations that fail as one whose allocation failed does (mitcad#80).
    pub out_of_memory: RefCell<BTreeSet<&'static str>>,
    /// Which targets a boolean touches, by index; all when None.
    pub touch: RefCell<Option<Vec<bool>>>,
    /// Pieces a cut or intersection leaves of each touched target: 1 unless
    /// set, 0 removes the target.
    pub split: Cell<Option<usize>>,
    /// Faces that booleans remove from their results.
    pub lose: RefCell<Vec<FaceName>>,
    /// Tools of faces are pockets (Cut) instead of bosses (Join).
    pub voids: Cell<bool>,
    /// What `face_geometry` returns, by face name (planar ends and starts
    /// of prisms otherwise).
    pub surfaces: RefCell<BTreeMap<String, SurfaceGeometry>>,
    /// Data exchange: the bodies `read_file` returns by path, and the files
    /// `write_file` wrote (path, format, body names and colours).
    pub files: RefCell<BTreeMap<String, Vec<ImportBody<MockShape>>>>,
    #[allow(clippy::type_complexity)]
    pub written: RefCell<Vec<(String, ExportFormat, Vec<(String, Option<[f64; 3]>)>)>>,
    /// The placements of each body of each `write_file` (mitcad#19).
    pub written_placements: RefCell<Vec<Vec<Vec<crate::exchange::BodyPlacement>>>>,
    /// The STEP and IGES unit of each `write_file` and the millimetres per
    /// unit of each `read_file` (P11: metric defaults).
    pub write_units: RefCell<Vec<crate::exchange::LengthUnit>>,
    pub read_units: RefCell<Vec<f64>>,
    /// Notes of the results of operations (P9): "fillet" -> what a smaller
    /// fillet would say; `notes` gives them for shapes the operation made.
    pub notes: RefCell<BTreeMap<&'static str, String>>,
    /// Profile features: what the extrude, revolve, hole and thread calls
    /// were asked for, one line each.
    pub specs: RefCell<Vec<String>>,
    /// Sketches (S1): what `curves_of` lists for any name on a shape that
    /// has faces.
    pub curves: RefCell<Vec<crate::kernel::Curve3>>,
    /// Paths of sweeps (F3): the curve `curves_of` gives for a name, before
    /// `curves`.
    pub named_curves: RefCell<BTreeMap<String, crate::kernel::Curve3>>,
    /// Recompute cancellation (P7): the operation that cancels the monitor
    /// when it is called, as a cancel that comes during an evaluation.
    pub cancel_on: RefCell<Option<(&'static str, Arc<RecomputeMonitor>)>>,
    /// Cancellation inside an operation (P7e): the monitor of the
    /// evaluation that runs (`interruptible`). With `stop_on_cancel`, an
    /// operation called once it is cancelled fails, as a kernel operation
    /// stopped inside.
    pub interrupt: RefCell<Option<Arc<RecomputeMonitor>>>,
    pub stop_on_cancel: Cell<bool>,
    /// 3MF export (mitcad#13): the tolerances `triangle_mesh` was asked for.
    pub mesh_tolerances: RefCell<Vec<crate::exchange::MeshTolerance>>,
    /// Joints (mitcad#55): what `edge_geometry` returns by edge name
    /// (lines otherwise), and `vertex_point` by vertex name.
    pub edge_curves: RefCell<BTreeMap<String, CurveGeometry>>,
    pub vertex_points: RefCell<BTreeMap<String, crate::datum::Vec3>>,
    /// Fillets repeated on a pattern's copies (mitcad#105): edge middles
    /// by edge name; `edge_middles` lists the shape's edges that have one
    /// (along y, 1 mm long).
    pub middles: RefCell<BTreeMap<String, [f64; 3]>>,
}

impl MockShape {
    /// A body of `faces` faces in a ring, as an importer would build it;
    /// `history` "mesh..." makes a mesh body.
    pub fn imported(history: &str, faces: usize) -> MockShape {
        let face =
            |i: usize| FaceName::new(FeatureUid(0), "import", Some(RoleKey::Index(i as u32)));
        MockShape {
            history: history.to_owned(),
            faces: (0..faces).map(face).collect(),
            edges: (0..faces)
                .map(|i| (face(i), face((i + 1) % faces)))
                .collect(),
            bounds: Some(BoundingBox {
                min: [0.0; 3],
                max: [10.0; 3],
            }),
            parts: Vec::new(),
        }
    }
}

impl MockKernel {
    fn call(&self, op: &'static str) -> Result<(), KernelError> {
        *self.calls.borrow_mut().entry(op).or_default() += 1;
        if let Some((on, monitor)) = &*self.cancel_on.borrow()
            && *on == op
        {
            monitor.cancel();
        }
        if self.stop_on_cancel.get()
            && self
                .interrupt
                .borrow()
                .as_ref()
                .is_some_and(|monitor| monitor.is_cancelled())
        {
            return Err(KernelError::failed(format!(
                "{op}: the operation was cancelled"
            )));
        }
        if self.out_of_memory.borrow().contains(op) {
            Err(KernelError::failed(format!(
                "{op}: {}",
                KernelError::OUT_OF_MEMORY
            )))
        } else if self.fail.borrow().contains(op) {
            Err(KernelError::failed(format!("{op} failed")))
        } else {
            Ok(())
        }
    }

    pub fn count(&self, op: &str) -> usize {
        self.calls.borrow().get(op).copied().unwrap_or(0)
    }

    fn dress(
        &self,
        op: &'static str,
        role: fn(FeatureUid, EdgeName) -> FaceName,
        feature: FeatureUid,
        body: &MockShape,
        edges: &[EdgeName],
        size: String,
    ) -> Result<MockShape, KernelError> {
        self.call(op)?;
        let mut shape = MockShape {
            history: format!("{op}({},{size})", body.history),
            ..body.clone()
        };
        for edge in edges {
            let matched: Vec<_> = body.matching_edges(edge).into_iter().cloned().collect();
            if matched.is_empty() {
                return Err(KernelError::failed(format!("the body has no edge {edge}")));
            }
            for (a, b) in matched {
                let face = role(feature, EdgeName::new(a.clone(), b.clone()));
                // The new face meets both faces of the edge and the faces at
                // its ends (those next to both).
                let next_to = |x: &FaceName| -> Vec<FaceName> {
                    body.edges
                        .iter()
                        .filter_map(|(p, q)| {
                            if p == x {
                                Some(q.clone())
                            } else if q == x {
                                Some(p.clone())
                            } else {
                                None
                            }
                        })
                        .collect()
                };
                let ends: Vec<FaceName> = next_to(&a)
                    .into_iter()
                    .filter(|f| next_to(&b).contains(f) && *f != a && *f != b)
                    .collect();
                shape.edges.retain(|e| *e != (a.clone(), b.clone()));
                for neighbour in [a, b].into_iter().chain(ends) {
                    shape.edges.push((face.clone(), neighbour));
                }
                shape.faces.push(face);
            }
        }
        Ok(shape)
    }
}

fn number(value: f64) -> String {
    format!("{value}")
}

/// The body's faces the references match.
fn faces_matching(body: &MockShape, references: &[FaceName]) -> Vec<FaceName> {
    body.faces
        .iter()
        .filter(|f| references.iter().any(|r| f.matches(r)))
        .cloned()
        .collect()
}

/// The edges of fillet or chamfer sets: the named ones and those of the faces.
fn set_edges<'a>(
    body: &MockShape,
    sets: impl Iterator<Item = (&'a [EdgeName], &'a [FaceName])>,
) -> Vec<EdgeName> {
    let mut edges = Vec::new();
    for (named, faces) in sets {
        edges.extend(named.iter().cloned());
        for (a, b) in &body.edges {
            if faces.iter().any(|f| a.matches(f) || b.matches(f)) {
                edges.push(EdgeName::new(a.clone(), b.clone()));
            }
        }
    }
    edges
}

impl Kernel for MockKernel {
    type Shape = MockShape;

    fn extrude(&self, spec: &ExtrudeSpec<'_>) -> Result<MockShape, KernelError> {
        self.call("extrude")?;
        let feature = spec.feature;
        let up = spec.direction[2] >= 0.0;
        let (z0, z1) = if up {
            (spec.start, spec.end)
        } else {
            (-spec.end, -spec.start)
        };
        let mut parts = Vec::new();
        for region in spec.regions {
            let start = FaceName::start(feature, region.key.clone());
            let end = FaceName::end(feature, region.key.clone());
            let mut faces = vec![start.clone(), end.clone()];
            let mut edges = Vec::new();
            for profile_loop in &region.loops {
                let sides: Vec<FaceName> = profile_loop
                    .segments
                    .iter()
                    .map(|s| FaceName::side(feature, s.key.clone()))
                    .collect();
                for (i, side) in sides.iter().enumerate() {
                    // Neighbours around the loop; a loop of one closed curve has a seam.
                    edges.push((side.clone(), sides[(i + 1) % sides.len()].clone()));
                    edges.push((side.clone(), start.clone()));
                    edges.push((side.clone(), end.clone()));
                }
                faces.extend(sides);
            }
            let (min, max) = region.bounds();
            parts.push(MockShape {
                history: format!(
                    "prism({feature}:{}..{})",
                    number(spec.start),
                    number(spec.end)
                ),
                faces,
                edges,
                bounds: Some(BoundingBox {
                    min: [min[0], min[1], z0],
                    max: [max[0], max[1], z1],
                }),
                parts: Vec::new(),
            });
        }
        if parts.len() == 1 {
            return Ok(parts.remove(0));
        }
        let all: Vec<&MockShape> = parts.iter().collect();
        let mut shape = MockShape::merged(
            format!(
                "prism({feature}:{}..{})",
                number(spec.start),
                number(spec.end)
            ),
            &all,
            &[],
        );
        shape.parts = parts;
        Ok(shape)
    }

    fn solids(&self, shape: &MockShape) -> Result<Vec<MockShape>, KernelError> {
        Ok(if shape.parts.is_empty() {
            vec![shape.clone()]
        } else {
            shape.parts.clone()
        })
    }

    fn boolean(
        &self,
        op: BooleanOp,
        targets: &[&MockShape],
        tool: &MockShape,
    ) -> Result<BooleanOutput<MockShape>, KernelError> {
        let name = match op {
            BooleanOp::Join => "join",
            BooleanOp::Cut => "cut",
            BooleanOp::Intersect => "intersect",
        };
        self.call(name)?;
        let touched = self
            .touch
            .borrow()
            .clone()
            .unwrap_or_else(|| vec![true; targets.len()]);
        let lost = self.lose.borrow().clone();
        let mut pieces = Vec::new();
        if op == BooleanOp::Join {
            let sources: Vec<usize> = (0..targets.len()).filter(|i| touched[*i]).collect();
            if !sources.is_empty() {
                let mut inputs: Vec<&MockShape> = sources.iter().map(|i| targets[*i]).collect();
                inputs.push(tool);
                let history = format!(
                    "join({})",
                    inputs
                        .iter()
                        .map(|s| s.history.as_str())
                        .collect::<Vec<_>>()
                        .join(",")
                );
                pieces.push(BooleanPiece {
                    shape: MockShape::merged(history, &inputs, &lost),
                    sources,
                });
            }
        } else {
            let verb = if op == BooleanOp::Cut {
                "cut"
            } else {
                "common"
            };
            for (i, target) in targets.iter().enumerate() {
                if !touched[i] {
                    continue;
                }
                let count = self.split.get().unwrap_or(1);
                for k in 0..count {
                    let mut history = format!("{verb}({},{})", target.history, tool.history);
                    if count > 1 {
                        history.push_str(&format!("#{k}"));
                    }
                    pieces.push(BooleanPiece {
                        shape: MockShape::merged(history, &[target, tool], &lost),
                        sources: vec![i],
                    });
                }
            }
        }
        Ok(BooleanOutput { pieces, touched })
    }

    fn fillet(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        sets: &[FilletSet<'_>],
        rolling_ball_corners: bool,
    ) -> Result<MockShape, KernelError> {
        let sizes: Vec<String> = sets
            .iter()
            .map(|set| {
                let size = match set.size {
                    FilletSize::Constant { radius } => number(radius),
                    size => format!("{size:?}"),
                };
                if set.curvature {
                    format!("{size},G2 weight {}", number(set.weight))
                } else {
                    size
                }
            })
            .collect();
        let corners = if rolling_ball_corners { "" } else { ",setback" };
        let edges = set_edges(body, sets.iter().map(|s| (s.edges, s.faces)));
        self.dress(
            "fillet",
            FaceName::fillet,
            feature,
            body,
            &edges,
            format!("{}{corners}", sizes.join(";")),
        )
    }

    fn chamfer(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        sets: &[Chamfer<'_>],
        corner: ChamferCorner,
    ) -> Result<MockShape, KernelError> {
        let sizes: Vec<String> = sets
            .iter()
            .map(|set| {
                format!(
                    "{:?}{}{}",
                    set.size,
                    set.reference.map_or(String::new(), |f| format!(",on {f}")),
                    if set.flip { ",flip" } else { "" }
                )
            })
            .collect();
        let edges = set_edges(body, sets.iter().map(|s| (s.edges, s.faces)));
        let corner = match corner {
            ChamferCorner::Chamfer => String::new(),
            other => format!(",{other:?} corners"),
        };
        self.dress(
            "chamfer",
            FaceName::chamfer,
            feature,
            body,
            &edges,
            format!("{}{corner}", sizes.join(";")),
        )
    }

    // Face operations (F2): the history records the operation; new faces
    // get the names the OCCT kernel gives them.

    fn shell(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        spec: &ShellSpec<'_>,
    ) -> Result<MockShape, KernelError> {
        self.call("shell")?;
        let mut shape = MockShape::merged(
            format!(
                "shell({},{},{})",
                body.history,
                number(spec.inside),
                number(spec.outside)
            ),
            &[body],
            &faces_matching(body, spec.faces),
        );
        for face in &body.faces {
            if !spec.faces.iter().any(|r| face.matches(r)) {
                shape.faces.push(FaceName::new(
                    feature,
                    "offset",
                    Some(RoleKey::Face(Box::new(face.clone()))),
                ));
            }
        }
        Ok(shape)
    }

    fn draft(
        &self,
        _feature: FeatureUid,
        body: &MockShape,
        spec: &DraftSpec<'_, MockShape>,
    ) -> Result<MockShape, KernelError> {
        self.call("draft")?;
        Ok(MockShape {
            history: format!("draft({},{})", body.history, number(spec.angle)),
            ..body.clone()
        })
    }

    fn offset_faces(
        &self,
        _feature: FeatureUid,
        body: &MockShape,
        _faces: &[FaceName],
        distance: f64,
    ) -> Result<MockShape, KernelError> {
        self.call("offset_faces")?;
        Ok(MockShape {
            history: format!("offset({},{})", body.history, number(distance)),
            ..body.clone()
        })
    }

    fn delete_faces(
        &self,
        _feature: FeatureUid,
        body: &MockShape,
        faces: &[FaceName],
    ) -> Result<MockShape, KernelError> {
        self.call("delete_faces")?;
        Ok(MockShape::merged(
            format!("delete({})", body.history),
            &[body],
            &faces_matching(body, faces),
        ))
    }

    fn split_body(
        &self,
        feature: FeatureUid,
        body: &MockShape,
        _tool: &ToolInput<'_, MockShape>,
        _extend: bool,
    ) -> Result<Vec<MockShape>, KernelError> {
        self.call("split_body")?;
        let count = self.split.get().unwrap_or(2);
        Ok((0..count)
            .map(|k| {
                let mut piece = MockShape {
                    history: format!("split({})#{k}", body.history),
                    ..body.clone()
                };
                piece.faces.push(FaceName::new(feature, "split", None));
                piece
            })
            .collect())
    }

    fn split_faces(
        &self,
        _feature: FeatureUid,
        body: &MockShape,
        faces: &[FaceName],
        _tool: &ToolInput<'_, MockShape>,
        _extend: bool,
    ) -> Result<MockShape, KernelError> {
        self.call("split_faces")?;
        let mut shape = MockShape {
            history: format!("split_faces({})", body.history),
            ..body.clone()
        };
        for face in faces_matching(body, faces) {
            shape.faces.retain(|f| *f != face);
            shape.faces.push(face.clone().piece(0));
            shape.faces.push(face.piece(1));
        }
        Ok(shape)
    }

    fn count_edges(&self, shape: &MockShape, edge: &EdgeName) -> usize {
        let count = shape.matching_edges(edge).len();
        match edge.index {
            Some(k) => usize::from(k < count as u32),
            None => count,
        }
    }

    fn count_faces(&self, shape: &MockShape, face: &FaceName) -> Result<usize, KernelError> {
        Ok(usize::from(shape.has_face(face)))
    }

    fn faces(&self, shape: &MockShape) -> Result<Vec<crate::kernel::FaceInfo>, KernelError> {
        Ok(shape
            .faces
            .iter()
            .map(|face| crate::kernel::FaceInfo {
                names: vec![face.to_string()],
                surface: "plane".to_owned(),
                area: 0.0,
            })
            .collect())
    }

    fn bounding_box(&self, shape: &MockShape) -> Result<Option<BoundingBox>, KernelError> {
        Ok(shape.bounds)
    }

    /// The bounding box's volume and centre, when [`MockKernel::measures`].
    fn mass_properties(&self, shape: &MockShape) -> Result<MassProperties, KernelError> {
        if !self.measures.get() {
            return Err(KernelError::Unsupported("mass properties"));
        }
        let b = shape
            .bounds
            .ok_or(KernelError::Unsupported("mass properties without bounds"))?;
        let size: [f64; 3] = std::array::from_fn(|i| b.max[i] - b.min[i]);
        Ok(MassProperties {
            volume: size[0] * size[1] * size[2],
            area: 2.0 * (size[0] * size[1] + size[1] * size[2] + size[2] * size[0]),
            center: std::array::from_fn(|i| (b.min[i] + b.max[i]) / 2.0),
        })
    }

    fn curves_of(
        &self,
        shape: &MockShape,
        name: &crate::topo::TopoName,
    ) -> Result<Vec<crate::kernel::Curve3>, KernelError> {
        self.call("curves_of")?;
        if let Some(curve) = self.named_curves.borrow().get(&name.to_string()) {
            return Ok(vec![curve.clone()]);
        }
        Ok(if shape.faces.is_empty() {
            Vec::new()
        } else {
            self.curves.borrow().clone()
        })
    }

    // FreeCAD import: faces and edges in the shape's order, their curves as
    // `curves_of` gives them; no vertices.
    fn indexed_element(
        &self,
        shape: &MockShape,
        kind: crate::kernel::ElementKind,
        index: usize,
    ) -> Result<Option<crate::kernel::IndexedElement>, KernelError> {
        use crate::kernel::ElementKind;
        let name = match kind {
            ElementKind::Face => shape
                .faces
                .get(index)
                .map(|f| crate::topo::TopoName::Face(f.clone())),
            ElementKind::Edge => shape
                .edges
                .get(index)
                .map(|(a, b)| crate::topo::TopoName::Edge(EdgeName::new(a.clone(), b.clone()))),
            ElementKind::Vertex => None,
        };
        let Some(name) = name else {
            return Ok(None);
        };
        Ok(Some(crate::kernel::IndexedElement {
            curves: self.curves_of(shape, &name)?,
            name: Some(name.to_string()),
        }))
    }

    // Data exchange: B-rep data is "<faces> <history>".

    fn import_brep(
        &self,
        feature: FeatureUid,
        data: &[u8],
        first_face: u32,
    ) -> Result<(MockShape, u32), KernelError> {
        self.call("import")?;
        let text = std::str::from_utf8(data).map_err(|_| KernelError::failed("not mock data"))?;
        let (count, history) = text
            .split_once(' ')
            .ok_or_else(|| KernelError::failed("not mock data"))?;
        let count: u32 = count
            .parse()
            .map_err(|_| KernelError::failed("not mock data"))?;
        let face = |i: u32| FaceName::new(feature, "import", Some(RoleKey::Index(first_face + i)));
        let shape = MockShape {
            history: format!(
                "import({feature}:{first_face}..{},{history})",
                first_face + count
            ),
            faces: (0..count).map(face).collect(),
            edges: (0..count)
                .map(|i| (face(i), face((i + 1) % count)))
                .collect(),
            bounds: Some(BoundingBox {
                min: [0.0; 3],
                max: [10.0; 3],
            }),
            parts: Vec::new(),
        };
        Ok((shape, count))
    }

    fn brep_data(&self, shape: &MockShape) -> Result<Vec<u8>, KernelError> {
        Ok(format!("{} {}", shape.faces.len(), shape.history).into_bytes())
    }

    fn compound(&self, shapes: &[MockShape]) -> Result<MockShape, KernelError> {
        let all: Vec<&MockShape> = shapes.iter().collect();
        let history: Vec<&str> = shapes.iter().map(|s| s.history.as_str()).collect();
        let mut shape = MockShape::merged(format!("compound({})", history.join(",")), &all, &[]);
        shape.parts = shapes.to_vec();
        Ok(shape)
    }

    fn body_kind(&self, shape: &MockShape) -> Result<BodyKind, KernelError> {
        Ok(if shape.faces.is_empty() {
            BodyKind::Empty
        } else if shape.history.starts_with("mesh") || shape.history.contains(",mesh") {
            BodyKind::Mesh
        } else if shape.history.contains(",sheet") {
            // An imported surface body ("import(F1:0..4,sheet)").
            BodyKind::Sheet
        } else {
            BodyKind::Solid
        })
    }

    fn read_file(
        &self,
        path: &str,
        options: &ReadOptions,
    ) -> Result<Vec<ImportBody<MockShape>>, KernelError> {
        self.read_units.borrow_mut().push(options.unit_mm);
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| KernelError::failed(format!("cannot read {path}")))
    }

    fn write_file(
        &self,
        path: &str,
        bodies: &[ExportBody<'_, MockShape>],
        options: &ExportOptions,
    ) -> Result<(), KernelError> {
        self.call("write")?;
        self.write_units.borrow_mut().push(options.unit);
        self.written_placements
            .borrow_mut()
            .push(bodies.iter().map(|b| b.placements.to_vec()).collect());
        self.written.borrow_mut().push((
            path.to_owned(),
            options.format,
            bodies
                .iter()
                .map(|b| (b.name.to_owned(), b.color))
                .collect(),
        ));
        Ok(())
    }

    // Transforms, patterns, mirrors, combine and primitives (F4).

    fn transform_shape(
        &self,
        shape: &MockShape,
        transform: &Transform,
        instance: Option<Instance>,
    ) -> Result<MockShape, KernelError> {
        self.call("transform")?;
        Ok(transformed(shape, transform, instance))
    }

    fn unite(&self, shapes: &[&MockShape]) -> Result<MockShape, KernelError> {
        self.call("unite")?;
        let history = format!(
            "unite({})",
            shapes
                .iter()
                .map(|s| s.history.as_str())
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut united = MockShape::merged(history, shapes, &[]);
        // The shapes stay separate solids, as if they did not touch.
        if shapes.len() > 1 {
            united.parts = shapes.iter().map(|s| (*s).clone()).collect();
        }
        Ok(united)
    }

    fn face_tool(
        &self,
        body: &MockShape,
        faces: &[FaceName],
    ) -> Result<(MockShape, BooleanOp), KernelError> {
        self.call("face_tool")?;
        let mut tool = MockShape {
            history: format!("faces({})", body.history),
            faces: Vec::new(),
            edges: Vec::new(),
            bounds: body.bounds,
            parts: Vec::new(),
        };
        for face in faces {
            let found: Vec<FaceName> = body
                .faces
                .iter()
                .filter(|f| f.matches(face))
                .cloned()
                .collect();
            if found.is_empty() {
                return Err(KernelError::failed(format!("the body has no face {face}")));
            }
            tool.faces.extend(found);
        }
        let op = if self.voids.get() {
            BooleanOp::Cut
        } else {
            BooleanOp::Join
        };
        Ok((tool, op))
    }

    /// The surfaces set in `surfaces`; else ends and starts of prisms are
    /// planes, as `face_plane` has them, and other faces unsupported.
    fn face_geometry(
        &self,
        shape: &MockShape,
        face: &FaceName,
    ) -> Result<SurfaceGeometry, KernelError> {
        if let Some(surface) = self.surfaces.borrow().get(&face.to_string()) {
            return Ok(surface.clone());
        }
        if !matches!(face.role.as_str(), "end" | "start") {
            return Err(KernelError::Unsupported("face geometry"));
        }
        let plane = self.face_plane(shape, face)?;
        Ok(SurfaceGeometry::Plane {
            origin: plane.origin,
            normal: plane.normal,
        })
    }

    fn primitive(&self, spec: &PrimitiveSpec) -> Result<MockShape, KernelError> {
        self.call("primitive")?;
        let feature = spec.feature;
        let (kind, size, roles): (&str, [f64; 3], &[&str]) = match spec.shape {
            PrimitiveShape::Box {
                length,
                width,
                height,
            } => (
                "box",
                [length, width, height],
                &["bottom", "top", "side0", "side1", "side2", "side3"],
            ),
            PrimitiveShape::Cylinder { radius, height } => (
                "cylinder",
                [radius, radius, height],
                &["bottom", "top", "side0"],
            ),
            PrimitiveShape::Sphere { radius } => ("sphere", [radius; 3], &["side0"]),
            PrimitiveShape::Torus {
                major_radius,
                minor_radius,
            } => (
                "torus",
                [major_radius, minor_radius, minor_radius],
                &["side0"],
            ),
        };
        let faces: Vec<FaceName> = roles
            .iter()
            .map(|role| FaceName::new(feature, role, None))
            .collect();
        let edges = faces
            .iter()
            .enumerate()
            .flat_map(|(i, a)| faces[i + 1..].iter().map(move |b| (a.clone(), b.clone())))
            .collect();
        let o = spec.frame.origin;
        Ok(MockShape {
            history: format!(
                "{kind}({feature}:{}x{}x{}@{},{},{})",
                number(size[0]),
                number(size[1]),
                number(size[2]),
                number(o[0]),
                number(o[1]),
                number(o[2])
            ),
            faces,
            edges,
            bounds: Some(BoundingBox {
                min: o,
                max: std::array::from_fn(|i| o[i] + size[i]),
            }),
            parts: Vec::new(),
        })
    }

    // Profile features (F1).

    fn extrude_feature(
        &self,
        spec: &ExtrudeFeatureSpec<'_, MockShape>,
    ) -> Result<MockShape, KernelError> {
        self.specs.borrow_mut().push(describe_extrude(spec));
        if let Ok(shape) = simple_extrude(self, spec) {
            return Ok(shape);
        }
        // A prism along the first side's distance (or 10 up to a target).
        let length = match spec.side1.end {
            SideEnd::Distance(d) => d,
            SideEnd::Target(_) => 10.0,
        };
        let extrusion = ExtrudeSpec {
            feature: spec.feature,
            frame: spec.frame,
            regions: spec.regions,
            direction: spec.direction,
            start: 0.0,
            end: length,
        };
        self.extrude(&extrusion)
    }

    fn revolve(&self, spec: &RevolveSpec<'_, MockShape>) -> Result<MockShape, KernelError> {
        self.call("revolve")?;
        self.specs.borrow_mut().push(format!(
            "revolve {} axis {:?} {:?} angles {} {:?} target {}",
            spec.feature,
            spec.axis.origin,
            spec.axis.direction,
            number(spec.angle1),
            spec.angle2,
            spec.target.is_some()
        ));
        let full = spec.target.is_none()
            && spec.angle1 + spec.angle2.unwrap_or(0.0) >= std::f64::consts::TAU - 1e-9;
        let mut faces = Vec::new();
        let mut edges = Vec::new();
        for region in spec.regions {
            let start = FaceName::start(spec.feature, region.key.clone());
            let end = FaceName::end(spec.feature, region.key.clone());
            if !full {
                faces.extend([start.clone(), end.clone()]);
            }
            for segment in region.loops.iter().flat_map(|l| &l.segments) {
                let side = FaceName::side(spec.feature, segment.key.clone());
                if !full {
                    edges.push((side.clone(), start.clone()));
                    edges.push((side.clone(), end.clone()));
                }
                faces.push(side);
            }
        }
        Ok(MockShape {
            history: format!("revolve({}:{})", spec.feature, number(spec.angle1)),
            faces,
            edges,
            bounds: None,
            parts: Vec::new(),
        })
    }

    fn hole_tool(&self, spec: &HoleSpec<'_, MockShape>) -> Result<MockShape, KernelError> {
        self.call("hole")?;
        let end = match &spec.end {
            HoleEnd::Distance(d) => format!("depth {}", number(*d)),
            HoleEnd::ThroughAll => format!("through {} bodies", spec.bodies.len()),
            HoleEnd::Target(_) => "target".to_owned(),
        };
        let mut text = format!(
            "hole {} at {:?} diameter {:.6} {:?} tip {:?} {end}",
            spec.feature,
            spec.positions
                .iter()
                .map(|p| (p.origin, p.direction))
                .collect::<Vec<_>>(),
            spec.diameter,
            spec.shape,
            spec.tip_angle.map(number)
        );
        if spec.taper != 0.0 {
            text += &format!(" taper {}", number(spec.taper));
        }
        self.specs.borrow_mut().push(text);
        let mut faces = Vec::new();
        let mut edges = Vec::new();
        for i in 0..spec.positions.len() {
            let face = |part: &str| FaceName::new(spec.feature, &format!("hole{i}.{part}"), None);
            faces.extend([face("wall"), face("tip")]);
            edges.push((face("wall"), face("tip")));
        }
        Ok(MockShape {
            history: format!("hole({}:{})", spec.feature, spec.positions.len()),
            faces,
            edges,
            bounds: None,
            parts: Vec::new(),
        })
    }

    fn modeled_thread(
        &self,
        body: &MockShape,
        spec: &ThreadSpec<'_>,
    ) -> Result<MockShape, KernelError> {
        self.call("thread")?;
        self.specs.borrow_mut().push(format!(
            "thread {} pitch {} diameters {:.4}/{:.4}/{:.4} angle {:.6} right {} part {:?}",
            spec.faces
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            number(spec.pitch),
            spec.major,
            spec.minor,
            spec.pitch_diameter,
            spec.angle,
            spec.right_handed,
            spec.part
        ));
        let mut shape = MockShape {
            history: format!("thread({},{})", body.history, number(spec.pitch)),
            ..body.clone()
        };
        for face in spec.faces {
            shape.faces.push(FaceName::new(
                spec.feature,
                "thread",
                Some(RoleKey::Face(Box::new(face.clone()))),
            ));
        }
        Ok(shape)
    }

    /// End caps lie on the top and bottom of the bounds.
    fn face_plane(&self, shape: &MockShape, face: &FaceName) -> Result<Plane, KernelError> {
        if !shape.has_face(face) {
            return Err(KernelError::failed(format!("the body has no face {face}")));
        }
        let bounds = shape.bounds.ok_or(KernelError::failed("no bounds"))?;
        let middle = |z: f64| {
            [
                (bounds.min[0] + bounds.max[0]) / 2.0,
                (bounds.min[1] + bounds.max[1]) / 2.0,
                z,
            ]
        };
        match face.role.as_str() {
            "end" => Ok(Plane {
                origin: middle(bounds.max[2]),
                normal: [0.0, 0.0, 1.0],
            }),
            "start" => Ok(Plane {
                origin: middle(bounds.min[2]),
                normal: [0.0, 0.0, -1.0],
            }),
            _ => Err(KernelError::failed(format!("face {face} is not planar"))),
        }
    }

    /// Side faces of closed curves and hole walls are cylinders along z,
    /// as wide as the bounds; hole walls are internal.
    fn face_cylinder(&self, shape: &MockShape, face: &FaceName) -> Result<Cylinder, KernelError> {
        if !shape.has_face(face) {
            return Err(KernelError::failed(format!("the body has no face {face}")));
        }
        let internal = face.role.ends_with(".wall");
        let closed = matches!(&face.key, Some(RoleKey::Segment(s)) if s.ends.is_none());
        if !internal && !(face.role == "side" && closed) {
            return Err(KernelError::failed(format!(
                "face {face} is not cylindrical"
            )));
        }
        let bounds = shape.bounds.unwrap_or(BoundingBox {
            min: [0.0; 3],
            max: [10.0; 3],
        });
        Ok(Cylinder {
            axis: Axis {
                origin: [
                    (bounds.min[0] + bounds.max[0]) / 2.0,
                    (bounds.min[1] + bounds.max[1]) / 2.0,
                    bounds.min[2],
                ],
                direction: [0.0, 0.0, 1.0],
            },
            radius: (bounds.max[0] - bounds.min[0]) / 2.0,
            length: bounds.max[2] - bounds.min[2],
            internal,
        })
    }

    // Sweeps, lofts, pipes, coils, ribs and webs (F3): shapes with the
    // OCCT facade's face names, the spec described in `specs`.

    fn sweep(&self, spec: &SweepSpec<'_>) -> Result<MockShape, KernelError> {
        self.call("sweep")?;
        let names: Vec<&str> = spec.path.iter().map(|p| p.name.as_str()).collect();
        self.specs.borrow_mut().push(format!(
            "sweep {} path {} extents {} {} {:?} twist {} taper {} rail {}",
            spec.feature,
            names.join(","),
            number(spec.extent1),
            number(spec.extent2),
            spec.orientation,
            number(spec.twist),
            number(spec.taper),
            spec.guide.map_or_else(
                || "none".to_owned(),
                |g| format!(
                    "{} {:?}",
                    g.rail
                        .iter()
                        .map(|p| p.name.as_str())
                        .collect::<Vec<_>>()
                        .join(","),
                    g.scaling
                )
            )
        ));
        let around = crate::features::path::is_closed(spec.path)
            && spec.extent1 + spec.extent2 >= 1.0 - 1e-9;
        let mut faces = Vec::new();
        let mut edges = Vec::new();
        for region in spec.regions {
            let caps = [
                FaceName::start(spec.feature, region.key.clone()),
                FaceName::end(spec.feature, region.key.clone()),
            ];
            if !around {
                faces.extend(caps.clone());
            }
            for segment in region.loops.iter().flat_map(|l| &l.segments) {
                let side = FaceName::side(spec.feature, segment.key.clone());
                if !around {
                    for cap in &caps {
                        edges.push((side.clone(), cap.clone()));
                    }
                }
                faces.push(side);
            }
        }
        Ok(MockShape {
            history: format!("sweep({}:{})", spec.feature, names.join(",")),
            faces,
            edges,
            bounds: None,
            parts: Vec::new(),
        })
    }

    fn loft(&self, spec: &LoftSpec<'_, MockShape>) -> Result<MockShape, KernelError> {
        self.call("loft")?;
        let describe = |section: &LoftSection<'_, MockShape>| match section {
            LoftSection::Region { region, .. } => format!("region {}", region.key),
            LoftSection::Face { face, .. } => format!("face {face}"),
            LoftSection::Point(p) => format!("point {p:?}"),
        };
        let names = |path: &[PathCurve]| {
            path.iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(",")
        };
        let mut text = format!(
            "loft {} sections {} ruled {} closed {} centerline {}",
            spec.feature,
            spec.sections
                .iter()
                .map(describe)
                .collect::<Vec<_>>()
                .join("; "),
            spec.ruled,
            spec.closed,
            spec.centerline.map_or_else(|| "none".to_owned(), names)
        );
        // Rails and end conditions only when there are any.
        if !spec.rails.is_empty() {
            let rails: Vec<String> = spec.rails.iter().map(|r| names(r)).collect();
            text += &format!(" rails {}", rails.join("; "));
        }
        for (which, end) in [("start", spec.start), ("end", spec.end)] {
            if !matches!(end.kind, LoftEndKind::Free | LoftEndKind::PointSharp) {
                text += &format!(
                    " {which} {:?} angle {} weight {}",
                    end.kind, end.angle, end.weight
                );
            }
        }
        self.specs.borrow_mut().push(text);
        let key = |section: &LoftSection<'_, MockShape>| match section {
            LoftSection::Region { region, .. } => Some(RoleKey::Region(region.key.clone())),
            LoftSection::Face { face, .. } => Some(RoleKey::Face(Box::new((*face).clone()))),
            LoftSection::Point(_) => None,
        };
        let mut faces = Vec::new();
        for section in &spec.sections {
            match section {
                LoftSection::Region { region, .. } => {
                    for segment in &region.loops[0].segments {
                        faces.push(FaceName::side(spec.feature, segment.key.clone()));
                    }
                    break;
                }
                LoftSection::Face { face, .. } => {
                    faces.push(FaceName::new(
                        spec.feature,
                        "side",
                        Some(RoleKey::Face(Box::new((*face).clone()))),
                    ));
                    break;
                }
                LoftSection::Point(_) => {}
            }
        }
        if !spec.closed {
            for (role, section) in [
                ("start", spec.sections.first()),
                ("end", spec.sections.last()),
            ] {
                if let Some(key) = section.and_then(key) {
                    faces.push(FaceName::new(spec.feature, role, Some(key)));
                }
            }
        }
        Ok(MockShape {
            history: format!("loft({}:{})", spec.feature, spec.sections.len()),
            faces,
            edges: Vec::new(),
            bounds: None,
            parts: Vec::new(),
        })
    }

    fn pipe(&self, spec: &PipeSpec<'_>) -> Result<MockShape, KernelError> {
        self.call("pipe")?;
        let names: Vec<&str> = spec.path.iter().map(|p| p.name.as_str()).collect();
        self.specs.borrow_mut().push(format!(
            "pipe {} path {} extents {} {} {:?} size {} thickness {:?}",
            spec.feature,
            names.join(","),
            number(spec.extent1),
            number(spec.extent2),
            spec.section,
            number(spec.size),
            spec.thickness
        ));
        let sides = match spec.section {
            PipeSection::Circular => 1,
            PipeSection::Square => 4,
            PipeSection::Triangular => 3,
        };
        let mut faces: Vec<FaceName> = (0..sides)
            .map(|i| FaceName::new(spec.feature, &format!("side{i}"), None))
            .collect();
        if spec.thickness.is_some() {
            faces.extend(
                (0..sides).map(|i| FaceName::new(spec.feature, &format!("inner{i}"), None)),
            );
        }
        faces.extend(["start", "end"].map(|role| FaceName::new(spec.feature, role, None)));
        Ok(MockShape {
            history: format!("pipe({}:{})", spec.feature, names.join(",")),
            faces,
            edges: Vec::new(),
            bounds: None,
            parts: Vec::new(),
        })
    }

    fn coil(&self, spec: &CoilSpec) -> Result<MockShape, KernelError> {
        self.call("coil")?;
        self.specs.borrow_mut().push(format!(
            "coil {} at {:?} axis {:?} diameter {} revolutions {} pitch {} angle {} spiral {} \
             clockwise {} {:?} {:?} size {}",
            spec.feature,
            spec.frame.origin,
            spec.frame.normal(),
            number(spec.diameter),
            number(spec.revolutions),
            number(spec.pitch),
            number(spec.angle),
            spec.spiral,
            spec.clockwise,
            spec.section,
            spec.position,
            number(spec.size)
        ));
        let mut faces = vec![
            FaceName::new(spec.feature, "start", None),
            FaceName::new(spec.feature, "end", None),
        ];
        faces.push(FaceName::new(spec.feature, "side0", None));
        Ok(MockShape {
            history: format!("coil({})", spec.feature),
            faces,
            edges: Vec::new(),
            bounds: None,
            parts: Vec::new(),
        })
    }

    fn rib(&self, spec: &RibSpec<'_, MockShape>) -> Result<MockShape, KernelError> {
        self.call("rib")?;
        let chains: Vec<String> = spec
            .chains
            .iter()
            .map(|c| {
                c.iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            })
            .collect();
        self.specs.borrow_mut().push(format!(
            "{} {} chains {} thickness {} {:?} depth {:?} flip {} bodies {}",
            if spec.web { "web" } else { "rib" },
            spec.feature,
            chains.join("; "),
            number(spec.thickness),
            spec.location,
            spec.depth,
            spec.flip,
            spec.bodies.len()
        ));
        let mut faces = Vec::new();
        for curve in spec.chains.iter().flatten() {
            let key = RoleKey::Segment(SegmentKey::closed(
                crate::ids::EntityUid::parse_curve(&curve.name).unwrap_or(crate::ids::EntityUid(0)),
            ));
            let role = if spec.web { "wall1" } else { "side" };
            faces.push(FaceName::new(spec.feature, role, Some(key)));
        }
        Ok(MockShape {
            history: format!("{}({})", if spec.web { "web" } else { "rib" }, spec.feature),
            faces,
            edges: Vec::new(),
            bounds: None,
            parts: Vec::new(),
        })
    }

    /// The points set in `vertex_points` (mitcad#55).
    fn vertex_point(
        &self,
        _shape: &MockShape,
        vertex: &crate::topo::VertexName,
    ) -> Result<crate::datum::Vec3, KernelError> {
        self.vertex_points
            .borrow()
            .get(&vertex.to_string())
            .copied()
            .ok_or_else(|| KernelError::failed(format!("the body has no vertex {vertex}")))
    }

    /// Every edge is a line along x from the bounds' low corner, unless
    /// `edge_curves` says otherwise.
    fn edge_geometry(
        &self,
        shape: &MockShape,
        edge: &EdgeName,
    ) -> Result<CurveGeometry, KernelError> {
        if shape.matching_edges(edge).is_empty() {
            return Err(KernelError::failed(format!("the body has no edge {edge}")));
        }
        if let Some(curve) = self.edge_curves.borrow().get(&edge.to_string()) {
            return Ok(curve.clone());
        }
        let start = shape.bounds.map_or([0.0; 3], |b| b.min);
        Ok(CurveGeometry::Line {
            start,
            end: [start[0] + 1.0, start[1], start[2]],
        })
    }

    /// Box letters of "Mock Sans" (any other family falls back to it): a
    /// 0.5 x 0.7 em block; `O`-like letters have one counter, `B` and `8`
    /// two, `C` and `S` a curved top (a quadratic piece); 0.7 em advance.
    fn font_glyphs(&self, request: &FontRequest<'_>) -> Result<FontGlyphs, KernelError> {
        self.call("font_glyphs")?;
        let rect = |x0: f64, y0: f64, x1: f64, y1: f64, ccw: bool| {
            let mut corners = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
            if !ccw {
                corners.reverse();
            }
            (0..4)
                .map(|i| vec![corners[i], corners[(i + 1) % 4]])
                .collect::<Vec<_>>()
        };
        let glyphs = request
            .text
            .chars()
            .map(|c| {
                let outer = rect(0.1, 0.0, 0.6, 0.7, true);
                let contours = match c {
                    ' ' | '\n' => Vec::new(),
                    'O' | 'o' | '0' | 'A' | 'a' | 'D' | 'e' | 'P' | 'R' | 'Q' | 'd' | 'b' => {
                        vec![outer, rect(0.25, 0.2, 0.45, 0.5, false)]
                    }
                    'B' | '8' => vec![
                        outer,
                        rect(0.25, 0.1, 0.45, 0.3, false),
                        rect(0.25, 0.4, 0.45, 0.6, false),
                    ],
                    'C' | 'S' => vec![vec![
                        vec![[0.1, 0.0], [0.6, 0.0]],
                        vec![[0.6, 0.0], [0.6, 0.7]],
                        vec![[0.6, 0.7], [0.35, 0.95], [0.1, 0.7]],
                        vec![[0.1, 0.7], [0.1, 0.0]],
                    ]],
                    _ => vec![outer],
                };
                Glyph {
                    advance: match c {
                        '\n' => 0.0,
                        ' ' => 0.35,
                        _ => 0.7,
                    },
                    contours,
                }
            })
            .collect();
        Ok(FontGlyphs {
            family: "Mock Sans".to_owned(),
            fallback: !(request.family.is_empty() || request.family == "Mock Sans"),
            ascender: 0.75,
            descender: -0.25,
            line_spacing: 1.2,
            glyphs,
        })
    }

    // Model warnings (P9).

    fn notes(&self, shape: &MockShape) -> Vec<String> {
        self.notes
            .borrow()
            .iter()
            .filter(|(op, _)| shape.history.starts_with(&format!("{op}(")))
            .map(|(_, note)| note.clone())
            .collect()
    }

    // The result store (P7d): a shape as JSON.

    fn shape_bytes(&self, shape: &MockShape) -> Result<Vec<u8>, KernelError> {
        self.call("shape_bytes")?;
        Ok(serde_json::to_vec(&StoredMock::new(shape)).expect("mock shapes serialize"))
    }

    fn shape_from_bytes(&self, bytes: &[u8]) -> Result<MockShape, KernelError> {
        self.call("shape_from_bytes")?;
        serde_json::from_slice::<StoredMock>(bytes)
            .map(StoredMock::shape)
            .map_err(|e| KernelError::failed(format!("not a stored mock shape: {e}")))
    }

    /// A kilobyte a face, so that tests can reckon with the memory budget.
    fn shape_memory(&self, shape: &MockShape) -> u64 {
        1000 * shape.faces.len() as u64
    }

    // Cancellation inside an operation (P7e).

    fn interruptible<R>(&self, monitor: &Arc<RecomputeMonitor>, f: impl FnOnce() -> R) -> R {
        *self.calls.borrow_mut().entry("interruptible").or_default() += 1;
        let outer = self.interrupt.replace(Some(monitor.clone()));
        let result = f();
        self.interrupt.replace(outer);
        result
    }

    // Triangle meshes for 3D printing (3MF export, mitcad#13).

    /// The shape's bounding box as a closed mesh of 12 triangles facing
    /// outwards; the tolerances go to `mesh_tolerances`.
    fn triangle_mesh(
        &self,
        shape: &MockShape,
        tolerance: &crate::exchange::MeshTolerance,
    ) -> Result<crate::exchange::TriangleMesh, KernelError> {
        self.call("triangle_mesh")?;
        self.mesh_tolerances.borrow_mut().push(*tolerance);
        let Some(bounds) = shape.bounds else {
            return Ok(crate::exchange::TriangleMesh::default());
        };
        let corner = |i: usize| {
            std::array::from_fn(|axis| {
                if (i >> axis) & 1 == 1 {
                    bounds.max[axis]
                } else {
                    bounds.min[axis]
                }
            })
        };
        let quads = [
            [0, 2, 3, 1],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 6, 7, 3],
            [0, 4, 6, 2],
            [1, 3, 7, 5],
        ];
        Ok(crate::exchange::TriangleMesh {
            vertices: (0..8).map(corner).collect(),
            triangles: quads
                .iter()
                .flat_map(|q: &[u32; 4]| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
                .collect(),
        })
    }

    // FreeCAD helices (mitcad#4).

    fn helix(&self, spec: &crate::sweeps::HelixSpec<'_>) -> Result<MockShape, KernelError> {
        self.call("helix")?;
        let growth = if spec.growth == 0.0 {
            String::new()
        } else {
            let freecad = if spec.freecad { " freecad" } else { "" };
            format!(
                " growth {} flip {}{freecad}",
                number(spec.growth),
                spec.flip
            )
        };
        self.specs.borrow_mut().push(format!(
            "helix {} axis {:?} {:?} pitch {} revolutions {} left {}{growth}",
            spec.feature,
            spec.origin,
            spec.direction,
            number(spec.pitch),
            number(spec.revolutions),
            spec.left_handed
        ));
        let mut faces = Vec::new();
        for region in spec.regions {
            faces.push(FaceName::start(spec.feature, region.key.clone()));
            faces.push(FaceName::end(spec.feature, region.key.clone()));
            for segment in region.loops.iter().flat_map(|l| &l.segments) {
                faces.push(FaceName::side(spec.feature, segment.key.clone()));
            }
        }
        Ok(MockShape {
            history: format!("helix({})", spec.feature),
            faces,
            edges: Vec::new(),
            bounds: None,
            parts: Vec::new(),
        })
    }

    // Definitions evaluated in parallel (the .f3d import, mitcad#95): a
    // kernel with the same settings, its own calls and requests.
    fn fork(&self) -> Option<Self> {
        Some(MockKernel {
            measures: self.measures.clone(),
            fail: self.fail.clone(),
            out_of_memory: self.out_of_memory.clone(),
            touch: self.touch.clone(),
            split: self.split.clone(),
            lose: self.lose.clone(),
            voids: self.voids.clone(),
            surfaces: self.surfaces.clone(),
            files: self.files.clone(),
            notes: self.notes.clone(),
            curves: self.curves.clone(),
            named_curves: self.named_curves.clone(),
            edge_curves: self.edge_curves.clone(),
            vertex_points: self.vertex_points.clone(),
            middles: self.middles.clone(),
            ..MockKernel::default()
        })
    }

    // Fillets repeated on a pattern's copies (mitcad#105).
    fn edge_middles(
        &self,
        shape: &MockShape,
    ) -> Result<Vec<crate::kernel::EdgeMiddle>, KernelError> {
        let middles = self.middles.borrow();
        Ok(shape
            .edges
            .iter()
            .filter_map(|(a, b)| {
                let name = EdgeName::new(a.clone(), b.clone()).to_string();
                middles.get(&name).map(|point| crate::kernel::EdgeMiddle {
                    name,
                    point: *point,
                    tangent: [0.0, 1.0, 0.0],
                    length: 1.0,
                })
            })
            .collect())
    }
}

/// A mock shape as the mock kernel stores it.
#[derive(serde::Serialize, serde::Deserialize)]
struct StoredMock {
    history: String,
    faces: Vec<FaceName>,
    edges: Vec<(FaceName, FaceName)>,
    bounds: Option<([f64; 3], [f64; 3])>,
    parts: Vec<StoredMock>,
}

impl StoredMock {
    fn new(shape: &MockShape) -> Self {
        Self {
            history: shape.history.clone(),
            faces: shape.faces.clone(),
            edges: shape.edges.clone(),
            bounds: shape.bounds.map(|b| (b.min, b.max)),
            parts: shape.parts.iter().map(Self::new).collect(),
        }
    }

    fn shape(self) -> MockShape {
        MockShape {
            history: self.history,
            faces: self.faces,
            edges: self.edges,
            bounds: self.bounds.map(|(min, max)| BoundingBox { min, max }),
            parts: self.parts.into_iter().map(Self::shape).collect(),
        }
    }
}

/// A copy of a mock shape moved by `transform`, faces renamed by `instance`.
fn transformed(shape: &MockShape, transform: &Transform, instance: Option<Instance>) -> MockShape {
    let rename = |face: &FaceName| match instance {
        Some(instance) => instance.face(face.clone()),
        None => face.clone(),
    };
    let bounds = shape.bounds.map(|b| {
        let corners: Vec<[f64; 3]> = b.corners().map(|c| transform.apply_point(c)).collect();
        BoundingBox {
            min: std::array::from_fn(|i| {
                corners.iter().map(|c| c[i]).fold(f64::INFINITY, f64::min)
            }),
            max: std::array::from_fn(|i| {
                corners
                    .iter()
                    .map(|c| c[i])
                    .fold(f64::NEG_INFINITY, f64::max)
            }),
        }
    });
    let t = transform.translation;
    let tag = match instance {
        Some(instance) => format!(",inst{}", instance.index),
        None => String::new(),
    };
    MockShape {
        history: format!(
            "moved({},{},{},{}{tag})",
            shape.history,
            number(round(t[0])),
            number(round(t[1])),
            number(round(t[2]))
        ),
        faces: shape.faces.iter().map(rename).collect(),
        edges: shape
            .edges
            .iter()
            .map(|(a, b)| (rename(a), rename(b)))
            .collect(),
        bounds,
        parts: shape
            .parts
            .iter()
            .map(|p| transformed(p, transform, instance))
            .collect(),
    }
}

/// Rounded to 1e-9 so that histories compare.
fn round(value: f64) -> f64 {
    let rounded = (value * 1e9).round() / 1e9;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// One line about an extrusion's spec, for tests to look at.
fn describe_extrude(spec: &ExtrudeFeatureSpec<'_, MockShape>) -> String {
    let start = match &spec.start {
        ExtrudeStart::Offset(offset) => format!("offset {}", number(*offset)),
        ExtrudeStart::Object(bound) => format!("object {}", describe_target(&bound.target)),
    };
    let side = |side: &ExtrudeSide<MockShape>| {
        let end = match &side.end {
            SideEnd::Distance(d) => format!("distance {}", number(*d)),
            SideEnd::Target(bound) => format!(
                "to {} offset {}",
                describe_target(&bound.target),
                number(bound.offset)
            ),
        };
        let thin = side
            .thin
            .map(|w| format!(" thin {:?} {}", w.location, number(w.thickness)))
            .unwrap_or_default();
        format!("{end} taper {}{thin}", number(side.taper))
    };
    let mut text = format!(
        "extrude {} along {:?} from {start}: {}",
        spec.feature,
        spec.direction,
        side(&spec.side1)
    );
    if let Some(two) = &spec.side2 {
        text.push_str(&format!("; {}", side(two)));
    }
    text
}

fn describe_target(target: &Target<MockShape>) -> String {
    match target {
        Target::Plane(plane) => format!("plane {:?} {:?}", plane.origin, plane.normal),
        Target::Face { face, extend, .. } => format!("face {face} extend {extend}"),
        Target::Body { body, through } => format!("body {} through {through}", body.history),
    }
}
