// SPDX-License-Identifier: MIT
//! CAM in the document (mitcad#1013, [`crate::cam`]): the commands on
//! setups, operations, machines and their tool libraries (each one undo
//! step, which recomputes nothing), and the queries that compute a
//! setup's toolpaths through the crate `mitcad-cam` and post-process them
//! to G-code.
//!
//! **Geometry.** A setup's work coordinates are resolved at the timeline
//! marker: Z from a plane, a planar face or an axis (the design's Z by
//! default), X from an axis or the plane's own X, the stock's box in
//! those axes (the bodies' bounding boxes grown by margins, or a stock
//! body's), and the origin at a point of the stock or of the model. An
//! operation's inputs become outlines in the work plane
//! ([`crate::cam::outline`]): a planar face's boundary from the kernel, a
//! sketch region's loops, edges and sketch curves chained, points for
//! holes; arcs about the tool axis stay arcs.
//!
//! **Out of date.** Each operation's fingerprint covers its definition,
//! its setup's, its tool, the resolved work frame and stock, its values
//! as the parameters evaluate them and the versions of the bodies and
//! sketches it reads. `cam_generate` records it in the operation; a
//! fingerprint that differs later marks the toolpath out of date.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

use super::{Document, ModelError, invalid};
use crate::cam::engine::{self, CamError, Program, ProgramOp, Toolpath};
use crate::cam::library::{self, CamLibrary};
use crate::cam::outline::{self, Chain, Seg, WorkFrame};
use crate::cam::{
    CamBody, CamInput, CamMachine, CamOperation, CamOperationInput, CamSetup, CamSetupInput,
    CamState, CamToolInput, CamValue, Entry, MAX_OPERATIONS, MAX_SETUPS, MachineOrigin,
    OperationKind, SetupRef, Stock, ValueKind, WcsOrigin, check_operation, check_setup,
};
use crate::datum::DatumPlane;
use crate::features::OccurrencePath;
use crate::features::geom_ref::GeomRef;
use crate::fingerprint::Fingerprint;
use crate::ids::{BodyUid, ComponentUid, EntityUid, OccurrenceUid};
use crate::joints::path_component;
use crate::kernel::Kernel;
use crate::topo::TopoName;
use crate::transform::Transform;
use mitcad_cam::geom::{Contour, Region};
use mitcad_cam::ops::{ContourOp, DrillOp, FaceOp, PocketOp, RestOp, Side};
// The 3-axis operations (phase 3).
use crate::exchange::MeshTolerance;
use mitcad_cam::TriMesh;
use mitcad_cam::ops::{RasterOp, Roughing3dOp, Surface, WaterlineOp};
use mitcad_vec3::{cross, norm, scale, sub};

// The stock simulation's query (mitcad#1013, phase 3).
mod simulate;

/// A setup's work coordinates and stock at the timeline marker.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SetupFrame {
    frame: WorkFrame,
    /// The stock's box in work coordinates.
    stock_min: [f64; 3],
    stock_max: [f64; 3],
    /// The highest work Z of the bodies it machines.
    bodies_top: f64,
    /// Where tool changes and travel happen (work Z).
    safe_z: f64,
}

/// The outlines and points an operation's inputs give, in work
/// coordinates.
#[derive(Debug, Default)]
struct Outlines {
    /// Closed outlines nested into regions, each with its height.
    regions: Vec<(Region, f64)>,
    /// The outer outlines and the open ones.
    contours: Vec<(Contour, f64)>,
    /// Every outline: the islands' too.
    all: Vec<(Contour, f64)>,
    points: Vec<[f64; 2]>,
    /// The outlines' extent in the work plane.
    extent: Option<([f64; 2], [f64; 2])>,
}

impl Outlines {
    fn widen(&mut self, contour: &Contour) {
        let mut points: Vec<[f64; 2]> = contour.vertices.iter().map(|v| v.pos()).collect();
        // An arc's own points, not only its ends.
        for segment in contour.segments() {
            if let Some((center, ccw)) = segment.arc {
                let sweep = engine::toolpath::arc_sweep(segment.start, segment.end, center, ccw);
                let start = (segment.start[1] - center[1]).atan2(segment.start[0] - center[0]);
                let radius = (segment.start[0] - center[0]).hypot(segment.start[1] - center[1]);
                // The sweep is signed: negative clockwise.
                for i in 1..16 {
                    let t = start + sweep * f64::from(i) / 16.0;
                    points.push([center[0] + radius * t.cos(), center[1] + radius * t.sin()]);
                }
            }
        }
        for p in points {
            let (lo, hi) = self.extent.get_or_insert((p, p));
            *lo = [lo[0].min(p[0]), lo[1].min(p[1])];
            *hi = [hi[0].max(p[0]), hi[1].max(p[1])];
        }
    }

    /// Closed outlines at a height: regions with islands.
    fn add_loops(&mut self, loops: Vec<Contour>, z: f64) {
        for region in outline::regions(&loops) {
            self.contours.push((region.outer.clone(), z));
            self.regions.push((region, z));
        }
        for contour in loops {
            self.widen(&contour);
            self.all.push((contour, z));
        }
    }

    fn add_chain(&mut self, chain: Chain) {
        if chain.contour.closed {
            self.add_loops(vec![chain.contour], chain.z);
        } else {
            self.widen(&chain.contour);
            self.contours.push((chain.contour.clone(), chain.z));
            self.all.push((chain.contour, chain.z));
        }
    }
}

/// Closed outlines all level at one height, or what is wrong.
fn level_loops(chains: Vec<Chain>, not_level: &str) -> Result<(Vec<Contour>, f64), String> {
    let z = chains.first().map(|c| c.z).unwrap_or_default();
    let mut loops = Vec::new();
    for chain in chains {
        if !chain.contour.closed {
            return Err("An outline of the operation does not close.".to_owned());
        }
        if !chain.level || (chain.z - z).abs() > outline::JOIN {
            return Err(not_level.to_owned());
        }
        loops.push(chain.contour);
    }
    Ok((loops, z))
}

/// The fingerprint as the file records it.
fn hex(fingerprint: u128) -> String {
    format!("{fingerprint:032x}")
}

/// An operation's state for a fingerprint.
fn state(op: &CamOperation, fingerprint: u128) -> &'static str {
    if op.suppressed {
        return "suppressed";
    }
    match &op.generated {
        None => "not_generated",
        Some(recorded) if *recorded == hex(fingerprint) => "up_to_date",
        Some(_) => "out_of_date",
    }
}

/// An engine operation read from its JSON with the extra options, and
/// computed.
fn run<T: DeserializeOwned>(
    mut value: Value,
    extra: &Map<String, Value>,
    compute: impl FnOnce(&T) -> Result<Toolpath, CamError>,
) -> Result<Toolpath, String> {
    if let Value::Object(map) = &mut value {
        for (key, option) in extra {
            map.insert(key.clone(), option.clone());
        }
    }
    let op: T = serde_json::from_value(value)
        .map_err(|_| "The operation's values cannot be read.".to_owned())?;
    compute(&op).map_err(|e| e.message)
}

/// The operation before `op` in its setup that cut with the tool a rest
/// machining is left by (its `from_tool`), not suppressed: its stock to
/// leave is what the bigger tool left on the walls.
fn rest_previous<'a>(setup: &'a CamSetup, op: &CamOperation) -> Option<&'a CamOperation> {
    let OperationKind::Rest {
        from_tool: Some(from),
    } = op.kind
    else {
        return None;
    };
    setup
        .operations
        .iter()
        .take_while(|o| o.id != op.id)
        .filter(|o| o.tool == from && !o.suppressed)
        .last()
}

/// One operation of a setup as computed.
#[derive(Debug, Clone)]
struct Computed {
    id: u32,
    name: String,
    fingerprint: u128,
    /// None for a suppressed operation.
    result: Option<Result<ProgramOp, String>>,
}

/// What [`Document::compute_cam_setup`] gives.
struct SetupRun<'a> {
    frame: Result<SetupFrame, String>,
    machine: Option<&'a CamMachine>,
    operations: Vec<Computed>,
}

const NO_MACHINE: &str = "Choose a machine for the setup: the tools are in its tool library.";

/// How far the meshes the 3-axis operations machine may be from the
/// bodies' surfaces (mm): the toolpaths follow the meshes.
pub const CAM_SURFACE_DEVIATION: f64 = 0.005;

impl<K: Kernel> Document<K> {
    /// The design's CAM: its machines and setups.
    pub fn cam(&self) -> &CamState {
        &self.state.cam
    }

    /// A setup's index by its id or name.
    pub fn cam_setup_index(&self, setup: &SetupRef) -> Result<usize, ModelError> {
        let setups = &self.state.cam.setups;
        match setup {
            SetupRef::Id(id) => setups.iter().position(|s| s.id == *id),
            SetupRef::Name(name) => setups
                .iter()
                .position(|s| s.name.trim().eq_ignore_ascii_case(name.trim())),
        }
        .ok_or_else(|| invalid("There is no such CAM setup."))
    }

    /// Commits a changed CAM state as one undo step that recomputes
    /// nothing; none when nothing changed.
    fn commit_cam<T>(&mut self, cam: CamState, label: &str, value: T) -> Result<T, ModelError> {
        if cam == self.state.cam {
            return Ok(value);
        }
        let label = label.to_owned();
        self.apply_with(move |state| {
            state.cam = cam;
            Ok((label, value, false))
        })
    }

    // Values and references.

    fn cam_value(&self, value: &Option<CamValue>, kind: ValueKind) -> Result<Option<f64>, String> {
        value
            .as_ref()
            .map(|v| v.evaluate(self.parameters(), kind))
            .transpose()
    }

    // Placements: CAM's references are read in their components'
    // coordinates and placed where the occurrence they were picked in
    // places them (see `CamInput`).

    /// The component a reference's geometry belongs to; none for fixed
    /// geometry and the origin's datums.
    fn cam_ref_owner(&self, reference: &GeomRef) -> Option<ComponentUid> {
        match reference {
            GeomRef::Face { body, .. }
            | GeomRef::Edge { body, .. }
            | GeomRef::Vertex { body, .. }
            | GeomRef::Body(body) => self.body_component(*body),
            GeomRef::Datum(feature)
            | GeomRef::SketchCurves {
                sketch: feature, ..
            }
            | GeomRef::SketchPoint {
                sketch: feature, ..
            }
            | GeomRef::Mount { feature, .. } => self.feature(*feature).map(|f| f.component),
            _ => None,
        }
    }

    /// The occurrences from the design's root that place what a CAM
    /// reference reads: the path it was picked in, which must end in the
    /// component that owns it (`owner`); when none was kept, the owner's
    /// only placement (the root's own geometry and a component placed
    /// nowhere: none). A component placed more than once needs the path.
    fn cam_path(
        &self,
        owner: Option<ComponentUid>,
        occurrence: &OccurrencePath,
    ) -> Result<Vec<OccurrenceUid>, String> {
        if !occurrence.is_empty() {
            let component = path_component(self.assembly(), ComponentUid::ROOT, &occurrence.0)
                .map_err(|_| "A placement a CAM input was picked in is gone.".to_owned())?;
            if owner.is_some_and(|owner| owner != component) {
                return Err(
                    "A CAM input is not in the component placed where it was picked.".to_owned(),
                );
            }
            return Ok(occurrence.0.clone());
        }
        let Some(owner) = owner.filter(|owner| !owner.is_root()) else {
            return Ok(Vec::new());
        };
        let mut paths = self.assembly().paths_to(owner);
        match paths.len() {
            0 => Ok(Vec::new()),
            1 => Ok(paths.remove(0)),
            _ => Err(
                "A CAM input is in a component placed more than once: pick it in one of its placements."
                    .to_owned(),
            ),
        }
    }

    /// From the coordinates of a reference's component to the design's
    /// (see [`Self::cam_path`]).
    fn cam_placement(
        &self,
        owner: Option<ComponentUid>,
        occurrence: &OccurrencePath,
    ) -> Result<Transform, String> {
        self.cam_path(owner, occurrence)
            .map(|path| self.path_transform(&path))
    }

    /// Where an operation's input is placed; the stock is in the design's
    /// coordinates.
    fn cam_input_placement(&self, input: &CamInput) -> Result<Transform, String> {
        let Some(occurrence) = input.occurrence() else {
            return Ok(Transform::IDENTITY);
        };
        let owner = match (input, input.source()) {
            (CamInput::Point { at, .. }, _) => self.cam_ref_owner(at),
            (_, (Some(body), _)) => self.body_component(body),
            (_, (_, Some(sketch))) => self.feature(sketch).map(|f| f.component),
            _ => None,
        };
        self.cam_placement(owner, occurrence)
    }

    /// A body of a setup with its placement.
    fn cam_body_placed(&self, body: &CamBody) -> Result<(BodyUid, Transform), String> {
        self.cam_placement(self.body_component(body.body), &body.occurrence)
            .map(|placement| (body.body, placement))
    }

    /// A point reference, or an axis's point (a hole's cylindrical face),
    /// in the design, placed by the occurrence it was picked in.
    fn cam_point(&self, at: &GeomRef, occurrence: &OccurrencePath) -> Result<[f64; 3], String> {
        let point = match self.resolve_point(at) {
            Ok(point) => point,
            Err(_) => self
                .resolve_axis(at)
                .map(|axis| axis.origin)
                .map_err(|_| "A point of the CAM setup or operation cannot be found.".to_owned())?,
        };
        let placement = self.cam_placement(self.cam_ref_owner(at), occurrence)?;
        Ok(placement.apply_point(point))
    }

    /// A direction of the work coordinates in the design: a plane's
    /// normal and X (`None` for an axis), else an axis's direction, as
    /// the occurrence places them; none when it is not found.
    fn cam_direction(
        &self,
        reference: &GeomRef,
        occurrence: &OccurrencePath,
        plane_first: bool,
    ) -> Option<([f64; 3], Option<[f64; 3]>)> {
        let placement = self
            .cam_placement(self.cam_ref_owner(reference), occurrence)
            .ok()?;
        if plane_first && let Ok(plane) = self.resolve_plane(reference) {
            return Some((
                placement.apply_vector(cross(plane.x_axis, plane.y_axis)),
                Some(placement.apply_vector(plane.x_axis)),
            ));
        }
        let axis = self.resolve_axis(reference).ok()?;
        Some((placement.apply_vector(axis.direction), None))
    }

    /// The bodies a setup machines, each with its placement: its own,
    /// else every body as placed in the design but the stock body.
    fn cam_setup_bodies(&self, setup: &CamSetup) -> Result<Vec<(BodyUid, Transform)>, String> {
        if !setup.bodies.is_empty() {
            return setup
                .bodies
                .iter()
                .map(|body| self.cam_body_placed(body))
                .collect();
        }
        let stock = setup
            .stock
            .body()
            .map(|body| {
                self.cam_path(self.body_component(body.body), &body.occurrence)
                    .map(|path| (body.body, path))
            })
            .transpose()?;
        Ok(self
            .instances()
            .into_iter()
            .filter(|i| {
                stock
                    .as_ref()
                    .is_none_or(|(body, path)| i.body != *body || i.path != *path)
            })
            .map(|i| (i.body, i.transform))
            .collect())
    }

    /// The placed bodies' bounding boxes in the axes of `axes` (its origin
    /// the model's).
    fn cam_box(
        &self,
        bodies: &[(BodyUid, Transform)],
        axes: &WorkFrame,
    ) -> Option<([f64; 3], [f64; 3])> {
        let mut found: Option<([f64; 3], [f64; 3])> = None;
        for (body, placement) in bodies {
            let Some(shape) = self.body_shape(*body) else {
                continue;
            };
            let Ok(Some(bounds)) = self.kernel().bounding_box(shape) else {
                continue;
            };
            for corner in bounds.corners() {
                let w = axes.to_work(placement.apply_point(corner));
                let (lo, hi) = found.get_or_insert((w, w));
                let low: [f64; 3] = std::array::from_fn(|k| lo[k].min(w[k]));
                let high: [f64; 3] = std::array::from_fn(|k| hi[k].max(w[k]));
                *lo = low;
                *hi = high;
            }
        }
        found
    }

    /// A setup's work coordinates and stock at the marker.
    fn cam_setup_frame(&self, setup: &CamSetup) -> Result<SetupFrame, String> {
        let params = self.parameters();
        let (z, plane_x) = match &setup.wcs.z {
            None => ([0.0, 0.0, 1.0], None),
            Some(reference) => self
                .cam_direction(reference, &setup.wcs.z_occurrence, true)
                .ok_or_else(|| {
                    "The work Z cannot be found: pick a planar face, a plane or an axis.".to_owned()
                })?,
        };
        let length = norm(z);
        if length < 1e-12 {
            return Err(
                "The work Z cannot be found: pick a planar face, a plane or an axis.".to_owned(),
            );
        }
        let z = scale(z, 1.0 / length);
        let hint = match &setup.wcs.x {
            Some(reference) => Some(
                self.cam_direction(reference, &setup.wcs.x_occurrence, false)
                    .ok_or_else(|| {
                        "The work X cannot be found: pick a straight edge or an axis.".to_owned()
                    })?
                    .0,
            ),
            None => plane_x.or(Some([1.0, 0.0, 0.0])),
        };
        let plane = DatumPlane::from_normal([0.0; 3], z, hint);
        let axes = WorkFrame {
            origin: [0.0; 3],
            x: plane.x_axis,
            y: plane.y_axis,
            z,
        };
        let parts = self.cam_box(&self.cam_setup_bodies(setup)?, &axes);
        let (min, max) = match &setup.stock {
            Stock::AroundBodies { side, top, bottom } => {
                let (lo, hi) =
                    parts.ok_or_else(|| "The setup has no bodies to machine.".to_owned())?;
                let side = side.evaluate(params, ValueKind::Length)?;
                let top = top.evaluate(params, ValueKind::Length)?;
                let bottom = bottom.evaluate(params, ValueKind::Length)?;
                (
                    [lo[0] - side, lo[1] - side, lo[2] - bottom],
                    [hi[0] + side, hi[1] + side, hi[2] + top],
                )
            }
            Stock::Body { body, occurrence } => {
                let placed = self.cam_body_placed(&CamBody {
                    body: *body,
                    occurrence: occurrence.clone(),
                })?;
                self.cam_box(&[placed], &axes).ok_or_else(|| {
                    "The stock body is not in the design at the timeline marker.".to_owned()
                })?
            }
        };
        if (0..3).any(|k| max[k] - min[k] <= 1e-9) {
            return Err("The stock is empty: give it margins or a body.".to_owned());
        }
        let origin = match &setup.wcs.origin {
            WcsOrigin::Stock { corner } => corner.of(min, max),
            WcsOrigin::Point { at, occurrence } => axes.to_work(self.cam_point(at, occurrence)?),
        };
        let frame = WorkFrame {
            origin: axes.to_model(origin),
            ..axes
        };
        let stock_min = sub(min, origin);
        let stock_max = sub(max, origin);
        let bodies_top = parts.map_or(stock_max[2], |(_, hi)| hi[2] - origin[2]);
        let safe_z = stock_max[2] + setup.safe_height.evaluate(params, ValueKind::Length)?;
        Ok(SetupFrame {
            frame,
            stock_min,
            stock_max,
            bodies_top,
            safe_z,
        })
    }

    /// The segments of the kernel's curves in the work plane, the curves
    /// in their component's coordinates placed by `placement`.
    fn cam_curve_segments(
        curves: &[crate::Curve3],
        placement: &Transform,
        frame: &WorkFrame,
    ) -> Vec<Seg> {
        curves
            .iter()
            .filter_map(outline::piece_of_curve)
            .flat_map(|piece| outline::work_segments(&piece.placed(placement), frame))
            .collect()
    }

    /// An operation's inputs as outlines and points in the work plane.
    fn cam_outlines(&self, op: &CamOperation, sf: &SetupFrame) -> Result<Outlines, String> {
        const GONE_BODY: &str =
            "A body the operation reads is not in the design at the timeline marker.";
        let mut out = Outlines::default();
        for input in &op.inputs {
            // Where the occurrence it was picked in places it.
            let placement = self.cam_input_placement(input)?;
            match input {
                CamInput::Stock => {
                    let c = Contour::rectangle(
                        [sf.stock_min[0], sf.stock_min[1]],
                        [sf.stock_max[0], sf.stock_max[1]],
                    );
                    out.add_loops(vec![c], sf.stock_max[2]);
                }
                CamInput::Face { body, face, .. } => {
                    let shape = self.body_shape(*body).ok_or_else(|| GONE_BODY.to_owned())?;
                    let curves: Vec<crate::Curve3> = self
                        .kernel()
                        .face_boundary(shape, face)
                        .map_err(|_| "A face of the operation cannot be read.".to_owned())?
                        .into_iter()
                        .map(|(_, curve)| curve)
                        .collect();
                    if curves.is_empty() {
                        return Err("A face of the operation is gone or not planar.".to_owned());
                    }
                    let chains =
                        outline::chains(Self::cam_curve_segments(&curves, &placement, &sf.frame));
                    let (loops, z) = level_loops(
                        chains,
                        "A face of the operation is not level with the work plane.",
                    )?;
                    out.add_loops(loops, z);
                }
                CamInput::Region { sketch, region, .. } => {
                    let (frame, profile) = self
                        .profile_region(*sketch, region)
                        .ok_or_else(|| "A sketch profile of the operation is gone.".to_owned())?;
                    let mut segs = Vec::new();
                    for profile_loop in &profile.loops {
                        for segment in &profile_loop.segments {
                            let piece = outline::piece_of_segment(&segment.geometry, &frame)?
                                .placed(&placement);
                            segs.extend(outline::work_segments(&piece, &sf.frame));
                        }
                    }
                    let (loops, z) = level_loops(
                        outline::chains(segs),
                        "A sketch profile of the operation is not level with the work plane.",
                    )?;
                    out.add_loops(loops, z);
                }
                CamInput::Edges { body, edges, .. } => {
                    let shape = self.body_shape(*body).ok_or_else(|| GONE_BODY.to_owned())?;
                    let mut curves = Vec::new();
                    for edge in edges {
                        let found = self
                            .kernel()
                            .curves_of(shape, &TopoName::Edge(edge.clone()))
                            .unwrap_or_default();
                        if found.is_empty() {
                            return Err("An edge of the operation is gone.".to_owned());
                        }
                        curves.extend(found);
                    }
                    let segs = Self::cam_curve_segments(&curves, &placement, &sf.frame);
                    for chain in outline::chains(segs) {
                        if !chain.level {
                            return Err(
                                "Edges of the operation are not level with the work plane."
                                    .to_owned(),
                            );
                        }
                        out.add_chain(chain);
                    }
                }
                CamInput::Curves { sketch, curves, .. } => {
                    let output = self
                        .sketch_output(*sketch)
                        .ok_or_else(|| "A sketch of the operation is gone or failed.".to_owned())?;
                    let mut segs = Vec::new();
                    for name in curves {
                        let uid = EntityUid::parse_curve(name)
                            .map_err(|_| "A sketch curve is named like c3.".to_owned())?;
                        let curve =
                            output.solved.curves.get(&uid).ok_or_else(|| {
                                "A sketch curve of the operation is gone.".to_owned()
                            })?;
                        let piece =
                            outline::piece_of_sketch_curve(curve, &output.frame).placed(&placement);
                        segs.extend(outline::work_segments(&piece, &sf.frame));
                    }
                    for chain in outline::chains(segs) {
                        if !chain.level {
                            return Err(
                                "Sketch curves of the operation are not level with the work plane."
                                    .to_owned(),
                            );
                        }
                        out.add_chain(chain);
                    }
                }
                CamInput::Point { at, occurrence } => {
                    let w = sf.frame.to_work(self.cam_point(at, occurrence)?);
                    out.points.push([w[0], w[1]]);
                }
                // A body's surfaces, read by the 3-axis operations
                // (`cam_surface`).
                CamInput::Body { .. } => {}
            }
        }
        Ok(out)
    }

    /// The surfaces a 3-axis operation machines, in the work coordinates:
    /// its bodies' meshes (the setup's bodies when it picked none), each
    /// within [`CAM_SURFACE_DEVIATION`] and placed by its occurrence.
    fn cam_surface(
        &self,
        setup: &CamSetup,
        sf: &SetupFrame,
        op: &CamOperation,
    ) -> Result<Surface, String> {
        let mut bodies = Vec::new();
        for input in &op.inputs {
            if let CamInput::Body { body, .. } = input {
                bodies.push((*body, self.cam_input_placement(input)?));
            }
        }
        if bodies.is_empty() {
            bodies = self.cam_setup_bodies(setup)?;
        }
        let tolerance = MeshTolerance {
            deviation: CAM_SURFACE_DEVIATION,
            angle: 10.0_f64.to_radians(),
            face_triangles: 0,
            max_edge: 0.0,
        };
        let mut mesh = TriMesh::default();
        for (body, placement) in bodies {
            let shape = self.body_shape(body).ok_or_else(|| {
                "A body the operation reads is not in the design at the timeline marker.".to_owned()
            })?;
            let part = self
                .kernel()
                .triangle_mesh(shape, &tolerance)
                .map_err(|_| "A body of the operation cannot be meshed.".to_owned())?;
            let base = mesh.vertices.len() as u32;
            mesh.vertices.extend(
                part.vertices
                    .iter()
                    .map(|p| sf.frame.to_work(placement.apply_point(*p))),
            );
            mesh.triangles
                .extend(part.triangles.iter().map(|t| t.map(|i| i + base)));
        }
        Surface::new(&mesh).map_err(|e| e.message)
    }

    /// The options of the later operations' entries, tabs and leads, as
    /// the engine's operations read them: `entry` `{kind, angle (degrees),
    /// radius}`, `tabs` `{count, width, height}`, `lead` `{radius}`.
    /// A helix without a radius turns at a quarter of the tool's
    /// diameter; tabs are laid out by their count.
    fn cam_engine_options(
        &self,
        op: &CamOperation,
        diameter: f64,
    ) -> Result<Map<String, Value>, String> {
        let params = self.parameters();
        let length = |v: &CamValue| v.evaluate(params, ValueKind::Length);
        let angle = |v: &CamValue| v.evaluate(params, ValueKind::Angle);
        let mut out = Map::new();
        match &op.entry {
            None => {}
            Some(Entry::Plunge) => {
                out.insert("entry".to_owned(), json!({"kind": "plunge"}));
            }
            Some(Entry::Ramp { angle: a }) => {
                out.insert(
                    "entry".to_owned(),
                    json!({"kind": "ramp", "angle": angle(a)?}),
                );
            }
            Some(Entry::Helix { radius, angle: a }) => {
                let radius = radius
                    .as_ref()
                    .map(length)
                    .transpose()?
                    .unwrap_or(diameter / 4.0);
                out.insert(
                    "entry".to_owned(),
                    json!({"kind": "helix", "angle": angle(a)?, "radius": radius}),
                );
            }
        }
        if let Some(tabs) = &op.tabs {
            out.insert(
                "tabs".to_owned(),
                json!({"layout": {"by": "count", "count": tabs.count},
                       "width": length(&tabs.width)?, "height": length(&tabs.height)?}),
            );
        }
        if let Some(lead) = &op.lead {
            out.insert("lead".to_owned(), json!({"radius": length(&lead.radius)?}));
        }
        Ok(out)
    }

    /// One operation's toolpath with its tool and speeds.
    fn cam_compute_operation(
        &self,
        setup: &CamSetup,
        sf: &SetupFrame,
        machine: &CamMachine,
        op: &CamOperation,
    ) -> Result<ProgramOp, String> {
        use ValueKind::{Length, Number};
        let tool = machine.tools.get(op.tool).ok_or_else(|| {
            "The operation's tool is not in the machine's tool library.".to_owned()
        })?;
        let engine_tool = tool.engine()?;
        let defaults = tool.cutting;
        let rpm = match self.cam_value(&op.cutting.spindle_rpm, Number)? {
            Some(v) => v,
            None => defaults.map(|c| c.spindle_rpm).ok_or_else(|| {
                "Set the operation's spindle speed: its tool has none.".to_owned()
            })?,
        };
        let feed = match self.cam_value(&op.cutting.feed, Length)? {
            Some(v) => v,
            None => defaults
                .map(|c| c.feed)
                .ok_or_else(|| "Set the operation's feed: its tool has none.".to_owned())?,
        };
        let plunge = match self.cam_value(&op.cutting.plunge_feed, Length)? {
            Some(v) => v,
            None => defaults
                .map(|c| c.plunge_feed)
                .ok_or_else(|| "Set the operation's plunge feed: its tool has none.".to_owned())?,
        };
        let outlines = self.cam_outlines(op, sf)?;
        let top = self
            .cam_value(&op.depths.top, Length)?
            .unwrap_or(sf.stock_max[2]);
        let clearance = self
            .cam_value(&op.heights.clearance, Length)?
            .unwrap_or(sf.safe_z);
        let retract = self
            .cam_value(&op.heights.retract, Length)?
            .unwrap_or((top + 2.0).min(clearance));
        let stepover = self.cam_value(&op.stepover, Length)?;
        let stock_to_leave = self.cam_value(&op.stock_to_leave, Length)?.unwrap_or(0.0);
        let step_default = match op.kind {
            OperationKind::Drill { peck: true, .. } => tool.diameter,
            OperationKind::Drill { .. } => tool.flute_length,
            // Waterline finishing's levels a tenth of the diameter apart.
            OperationKind::WaterlineFinish { .. } => tool.diameter / 10.0,
            _ => tool.flute_length.min(tool.diameter),
        };
        let step_down = self
            .cam_value(&op.depths.step_down, Length)?
            .unwrap_or(step_default);
        let bottom_given = self.cam_value(&op.depths.bottom, Length)?;
        let heights = json!({"clearance": clearance, "retract": retract});
        let cutting = json!({"spindle_rpm": rpm, "feed": feed, "plunge_feed": plunge});
        let depths = |bottom: f64| json!({"top": top, "bottom": bottom, "step_down": step_down});
        let extra = self.cam_engine_options(op, tool.diameter)?;
        let toolpath = match &op.kind {
            OperationKind::Facing => {
                let (min, max) = outlines.extent.unwrap_or((
                    [sf.stock_min[0], sf.stock_min[1]],
                    [sf.stock_max[0], sf.stock_max[1]],
                ));
                let bottom = match bottom_given {
                    Some(bottom) => bottom,
                    None if sf.bodies_top < top - 1e-6 => sf.bodies_top,
                    None => {
                        return Err(
                            "Nothing to face: set the facing's bottom below the stock's top."
                                .to_owned(),
                        );
                    }
                };
                run::<FaceOp>(
                    json!({"min": min, "max": max, "depths": depths(bottom), "heights": heights,
                           "stepover": stepover, "cutting": cutting}),
                    &extra,
                    |o| engine::ops::face(o, &engine_tool),
                )?
            }
            OperationKind::Contour { side } => {
                let source = if *side == Side::On {
                    &outlines.all
                } else {
                    &outlines.contours
                };
                if source.is_empty() {
                    return Err(
                        "The contour has no outlines: pick faces, profiles, edges or sketch curves."
                            .to_owned(),
                    );
                }
                let contours: Vec<&Contour> = source.iter().map(|(c, _)| c).collect();
                let bottom = bottom_given.unwrap_or(sf.stock_min[2]);
                run::<ContourOp>(
                    json!({"contours": contours, "side": side, "direction": op.direction,
                           "depths": depths(bottom), "heights": heights,
                           "stock_to_leave": stock_to_leave, "cutting": cutting}),
                    &extra,
                    |o| engine::ops::contour(o, &engine_tool),
                )?
            }
            OperationKind::Pocket | OperationKind::Adaptive | OperationKind::Rest { .. } => {
                if outlines.regions.is_empty() {
                    return Err(
                        "The operation has no region to clear: pick faces or sketch profiles."
                            .to_owned(),
                    );
                }
                let mut all = Toolpath::default();
                for (region, z) in &outlines.regions {
                    let bottom = match bottom_given {
                        Some(bottom) => bottom,
                        None if *z < top - 1e-6 => *z,
                        None => {
                            return Err("The operation has no depth: set its bottom.".to_owned());
                        }
                    };
                    let value = json!({"region": region, "stepover": stepover,
                                       "direction": op.direction, "depths": depths(bottom),
                                       "heights": heights, "stock_to_leave": stock_to_leave,
                                       "cutting": cutting});
                    let path = match &op.kind {
                        OperationKind::Pocket => run::<PocketOp>(value, &extra, |o| {
                            engine::ops::pocket(o, &engine_tool)
                        })?,
                        // Adaptive clearing is the engine's pocket with its
                        // `adaptive` option (phase 2, part b), its settings
                        // the engine's defaults.
                        OperationKind::Adaptive => {
                            let mut value = value;
                            value["adaptive"] = json!({});
                            run::<PocketOp>(value, &extra, |o| {
                                engine::ops::pocket(o, &engine_tool)
                            })?
                        }
                        // Rest machining: the bigger tool by its number in
                        // the machine's tool library, the stock it left
                        // from the operation before that cut with it.
                        OperationKind::Rest { from_tool } => {
                            let from = from_tool.ok_or_else(|| {
                                "Choose the bigger tool the rest machining follows.".to_owned()
                            })?;
                            let bigger = machine.tools.get(from).ok_or_else(|| {
                                "The bigger tool is not in the machine's tool library.".to_owned()
                            })?;
                            let left = match rest_previous(setup, op) {
                                Some(o) => {
                                    self.cam_value(&o.stock_to_leave, Length)?.unwrap_or(0.0)
                                }
                                None => 0.0,
                            };
                            let mut value = value;
                            value["previous_diameter"] = json!(bigger.diameter);
                            value["previous_stock_to_leave"] = json!(left);
                            run::<RestOp>(value, &extra, |o| engine::ops::rest(o, &engine_tool))?
                        }
                        _ => unreachable!("a clearing operation"),
                    };
                    all.moves.extend(path.moves);
                }
                all
            }
            // 3-axis machining of the bodies' surfaces: once per boundary
            // picked, else within the stock's outline (roughing, raster)
            // or round everything (waterline).
            OperationKind::Roughing3d { .. }
            | OperationKind::RasterFinish { .. }
            | OperationKind::WaterlineFinish { .. } => {
                let surface = self.cam_surface(setup, sf, op)?;
                let bottom = bottom_given.unwrap_or(sf.stock_min[2]);
                let boundaries: Vec<Value> = if !outlines.regions.is_empty() {
                    outlines.regions.iter().map(|(r, _)| json!(r)).collect()
                } else if matches!(op.kind, OperationKind::WaterlineFinish { .. }) {
                    vec![Value::Null]
                } else {
                    let stock = Contour::rectangle(
                        [sf.stock_min[0], sf.stock_min[1]],
                        [sf.stock_max[0], sf.stock_max[1]],
                    );
                    vec![json!(Region::new(stock, Vec::new()))]
                };
                let degrees = |v: &Option<CamValue>| self.cam_value(v, ValueKind::Angle);
                let mut all = Toolpath::default();
                for boundary in boundaries {
                    let path = match &op.kind {
                        OperationKind::Roughing3d { adaptive } => {
                            let adaptive = if *adaptive { json!({}) } else { Value::Null };
                            run::<Roughing3dOp>(
                                json!({"boundary": boundary, "stepover": stepover,
                                       "direction": op.direction, "depths": depths(bottom),
                                       "heights": heights, "stock_to_leave": stock_to_leave,
                                       "cutting": cutting, "adaptive": adaptive}),
                                &extra,
                                |o| engine::ops::roughing(o, &surface, &engine_tool),
                            )?
                        }
                        OperationKind::RasterFinish {
                            angle: pass_angle,
                            slope,
                        } => run::<RasterOp>(
                            json!({"boundary": boundary,
                                   "angle": degrees(pass_angle)?.unwrap_or(0.0),
                                   "stepover": stepover, "max_slope": degrees(slope)?,
                                   "top": top, "bottom": bottom, "heights": heights,
                                   "stock_to_leave": stock_to_leave, "cutting": cutting}),
                            &extra,
                            |o| engine::ops::raster(o, &surface, &engine_tool),
                        )?,
                        OperationKind::WaterlineFinish { slope } => run::<WaterlineOp>(
                            json!({"boundary": boundary, "depths": depths(bottom),
                                   "heights": heights, "min_slope": degrees(slope)?,
                                   "stock_to_leave": stock_to_leave,
                                   "direction": op.direction, "cutting": cutting}),
                            &extra,
                            |o| engine::ops::waterline(o, &surface, &engine_tool),
                        )?,
                        _ => unreachable!("a 3-axis operation"),
                    };
                    all.moves.extend(path.moves);
                }
                all
            }
            OperationKind::Drill {
                peck,
                chip_break,
                canned_cycle,
            } => {
                if outlines.points.is_empty() {
                    return Err(
                        "The drilling has no holes: pick points, hole edges or hole faces."
                            .to_owned(),
                    );
                }
                let bottom = bottom_given.unwrap_or(sf.stock_min[2]);
                run::<DrillOp>(
                    json!({"points": outlines.points, "depths": depths(bottom),
                           "heights": heights, "peck": peck, "chip_break": chip_break,
                           "canned_cycle": canned_cycle,
                           "cutting": cutting}),
                    &extra,
                    |o| engine::ops::drill(o, &engine_tool),
                )?
            }
        };
        Ok(ProgramOp {
            name: op.name.clone(),
            tool: engine_tool,
            spindle_rpm: rpm,
            spindle: op.spindle,
            coolant: op.coolant,
            toolpath,
        })
    }

    /// An operation's fingerprint (see the module documentation).
    fn cam_fingerprint(
        &self,
        setup: &CamSetup,
        sf: &Result<SetupFrame, String>,
        machine: Option<&CamMachine>,
        op: &CamOperation,
    ) -> u128 {
        let mut f = Fingerprint::new("mitcad-cam-operation");
        let mut op_def = op.clone();
        op_def.generated = None;
        op_def.suppressed = false;
        op_def.name.clear();
        let mut setup_def = setup.clone();
        setup_def.operations.clear();
        setup_def.name.clear();
        // The eye is display state.
        setup_def.visible = true;
        let _ = serde_json::to_writer(&mut f, &(&setup_def, &op_def));
        let tool = machine.and_then(|m| m.tools.get(op.tool));
        let _ = serde_json::to_writer(&mut f, &(machine.map(|m| &m.machine), tool));
        // A rest machining follows the bigger tool and what it left.
        if let OperationKind::Rest {
            from_tool: Some(from),
        } = op.kind
        {
            let bigger = machine.and_then(|m| m.tools.get(from));
            let left = rest_previous(setup, op).map(|o| &o.stock_to_leave);
            let _ = serde_json::to_writer(&mut f, &(bigger, left));
        }
        // A 3-axis operation without bodies of its own machines the
        // setup's: their versions and placements.
        if op.kind.machines_surfaces()
            && !op.inputs.iter().any(|i| matches!(i, CamInput::Body { .. }))
        {
            match self.cam_setup_bodies(setup) {
                Ok(bodies) => {
                    for (body, placement) in bodies {
                        f.option(self.body_version(body), |f, v| {
                            f.u128(v);
                        });
                        for c in placement
                            .linear
                            .iter()
                            .flatten()
                            .chain(&placement.translation)
                        {
                            f.f64(*c);
                        }
                    }
                }
                Err(e) => {
                    f.str(&e);
                }
            }
        }
        match sf {
            Ok(sf) => {
                let frame = sf.frame;
                for v in [
                    frame.origin,
                    frame.x,
                    frame.y,
                    frame.z,
                    sf.stock_min,
                    sf.stock_max,
                ] {
                    for c in v {
                        f.f64(c);
                    }
                }
                f.f64(sf.bodies_top).f64(sf.safe_z);
            }
            Err(e) => {
                f.str(e);
            }
        }
        let params = self.parameters();
        for (value, kind) in setup.values().into_iter().chain(op.values()) {
            match value.evaluate(params, kind) {
                Ok(v) => f.f64(v),
                Err(e) => f.str(&e),
            };
        }
        for input in &op.inputs {
            let (body, sketch) = input.source();
            if let Some(body) = body {
                f.option(self.body_version(body), |f, v| {
                    f.u128(v);
                });
            }
            if let Some(sketch) = sketch {
                let version = self.result.sketches.get(&sketch).map(|s| s.version);
                f.option(version, |f, v| {
                    f.u128(v);
                });
            }
            // Where its occurrence places it: moving the occurrence moves
            // the outline (nothing for the design's own coordinates).
            match self.cam_input_placement(input) {
                Ok(placement) if placement.is_identity() => {}
                Ok(placement) => {
                    for c in placement
                        .linear
                        .iter()
                        .flatten()
                        .chain(&placement.translation)
                    {
                        f.f64(*c);
                    }
                }
                Err(e) => {
                    f.str(&e);
                }
            }
            if let CamInput::Point { at, occurrence } = input {
                match self.cam_point(at, occurrence) {
                    Ok(p) => {
                        for c in p {
                            f.f64(c);
                        }
                    }
                    Err(e) => {
                        f.str(&e);
                    }
                }
            }
        }
        f.finish()
    }

    /// Computes a setup's operations (all, or those of `only`).
    fn compute_cam_setup(&self, index: usize, only: Option<&[u32]>) -> SetupRun<'_> {
        let setup = &self.state.cam.setups[index];
        let machine = setup
            .machine
            .as_deref()
            .and_then(|m| self.state.cam.machine(m));
        let frame = self.cam_setup_frame(setup);
        let operations = setup
            .operations
            .iter()
            .filter(|op| only.is_none_or(|ids| ids.contains(&op.id)))
            .map(|op| {
                let fingerprint = self.cam_fingerprint(setup, &frame, machine, op);
                let result = (!op.suppressed).then(|| match (&frame, machine) {
                    (Err(e), _) => Err(e.clone()),
                    (Ok(_), None) => Err(NO_MACHINE.to_owned()),
                    (Ok(sf), Some(m)) => self.cam_compute_operation(setup, sf, m, op),
                });
                Computed {
                    id: op.id,
                    name: op.name.clone(),
                    fingerprint,
                    result,
                }
            })
            .collect();
        SetupRun {
            frame,
            machine,
            operations,
        }
    }

    /// The program of a computed setup; refused when an operation failed.
    fn cam_program(setup: &CamSetup, run: &SetupRun<'_>) -> Result<Program, String> {
        let sf = run.frame.as_ref().map_err(Clone::clone)?;
        let mut program = Program::new(&setup.name, sf.safe_z);
        program.work_offset = setup.work_offset;
        for computed in &run.operations {
            match &computed.result {
                None => {}
                Some(Ok(op)) => program.operations.push(op.clone()),
                Some(Err(e)) => return Err(format!("{}: {e}", computed.name)),
            }
        }
        if program.operations.is_empty() {
            return Err("The setup has no operations to write.".to_owned());
        }
        Ok(program)
    }

    /// The machine check's findings of a computed program, when the setup
    /// has a machine and the work zero's machine position.
    fn cam_findings(setup: &CamSetup, run: &SetupRun<'_>, program: &Program) -> Value {
        let (Some(machine), Some(origin)) = (run.machine, setup.placement) else {
            return json!([]);
        };
        let Ok(placement) =
            serde_json::from_value::<engine::Placement>(json!({"work_origin": origin}))
        else {
            return json!([]);
        };
        json!(engine::check(&machine.machine, program, &placement))
    }

    /// The post settings of a setup, with its probing option.
    fn cam_post_settings(setup: &CamSetup) -> engine::PostSettings {
        let mut value = serde_json::to_value(&setup.post).unwrap_or_else(|_| json!({}));
        if setup.probe_work_zero
            && let Value::Object(map) = &mut value
        {
            map.insert("probe_work_zero".to_owned(), json!(true));
        }
        serde_json::from_value(value).unwrap_or_else(|_| setup.post.clone())
    }

    /// A setup's work frame as JSON (model coordinates), or its error.
    fn cam_frame_json(frame: &Result<SetupFrame, String>) -> Value {
        match frame {
            Ok(sf) => json!({
                "origin": sf.frame.origin,
                "x": sf.frame.x,
                "y": sf.frame.y,
                "z": sf.frame.z,
                "stock_min": sf.stock_min,
                "stock_max": sf.stock_max,
                "bodies_top": sf.bodies_top,
                "safe_z": sf.safe_z,
            }),
            Err(e) => json!({"error": e}),
        }
    }

    // Commands.

    /// What is wrong with a setup in this design: its machine, bodies,
    /// references and values.
    fn check_cam_setup(&self, cam: &CamState, setup: &CamSetup) -> Result<(), ModelError> {
        check_setup(setup).map_err(invalid)?;
        if let Some(machine) = &setup.machine
            && cam.machine(machine).is_none()
        {
            return Err(invalid(
                "The design keeps no such machine: put the machine in first.",
            ));
        }
        let mut bodies = setup.bodies.clone();
        bodies.extend(setup.stock.body());
        if bodies.iter().any(|b| self.body_shape(b.body).is_none()) {
            return Err(invalid(
                "A body of the setup is not in the design at the timeline marker.",
            ));
        }
        for body in &bodies {
            self.cam_body_placed(body).map_err(invalid)?;
        }
        for (value, kind) in setup.values() {
            value.evaluate(self.parameters(), kind).map_err(invalid)?;
        }
        if let Some(z) = &setup.wcs.z {
            self.cam_placement(self.cam_ref_owner(z), &setup.wcs.z_occurrence)
                .map_err(invalid)?;
            if self.resolve_plane(z).is_err() && self.resolve_axis(z).is_err() {
                return Err(invalid(
                    "The work Z cannot be found: pick a planar face, a plane or an axis.",
                ));
            }
        }
        if let Some(x) = &setup.wcs.x {
            self.cam_placement(self.cam_ref_owner(x), &setup.wcs.x_occurrence)
                .map_err(invalid)?;
            if self.resolve_axis(x).is_err() {
                return Err(invalid(
                    "The work X cannot be found: pick a straight edge or an axis.",
                ));
            }
        }
        if let WcsOrigin::Point { at, occurrence } = &setup.wcs.origin {
            self.cam_point(at, occurrence).map_err(invalid)?;
        }
        Ok(())
    }

    /// What is wrong with an operation in its setup: its tool, inputs and
    /// values.
    fn check_cam_operation(
        &self,
        cam: &CamState,
        setup: &CamSetup,
        op: &CamOperation,
    ) -> Result<(), ModelError> {
        check_operation(op).map_err(invalid)?;
        if let Some(machine) = setup.machine.as_deref().and_then(|m| cam.machine(m))
            && machine.tools.get(op.tool).is_none()
        {
            return Err(invalid(format!(
                "The setup's machine has no tool {}.",
                op.tool
            )));
        }
        for input in &op.inputs {
            let (body, sketch) = input.source();
            if body.is_some_and(|b| self.body_shape(b).is_none()) {
                return Err(invalid(
                    "A body the operation reads is not in the design at the timeline marker.",
                ));
            }
            if sketch.is_some_and(|s| self.sketch_output(s).is_none()) {
                return Err(invalid(
                    "A sketch the operation reads is not evaluated at the timeline marker.",
                ));
            }
            self.cam_input_placement(input).map_err(invalid)?;
            if let CamInput::Point { at, occurrence } = input {
                self.cam_point(at, occurrence).map_err(invalid)?;
            }
        }
        for (value, kind) in op.values() {
            value.evaluate(self.parameters(), kind).map_err(invalid)?;
        }
        if let Some(Entry::Helix { .. } | Entry::Ramp { .. }) = &op.entry
            && matches!(
                op.kind,
                OperationKind::Drill { .. }
                    | OperationKind::Facing
                    | OperationKind::RasterFinish { .. }
                    | OperationKind::WaterlineFinish { .. }
            )
        {
            return Err(invalid(
                "Ramp and helix entries are for contours and the clearing operations.",
            ));
        }
        if op.tabs.is_some() && !matches!(op.kind, OperationKind::Contour { .. }) {
            return Err(invalid("Holding tabs are for contours."));
        }
        Ok(())
    }

    /// Adds a setup (`cam_setup_add`): the fields given, the rest by
    /// default (the design's only machine, when it keeps one). Returns its
    /// id.
    pub fn add_cam_setup(&mut self, input: CamSetupInput) -> Result<u32, ModelError> {
        let mut cam = self.state.cam.clone();
        if cam.setups.len() >= MAX_SETUPS {
            return Err(invalid(format!(
                "A design has at most {MAX_SETUPS} setups."
            )));
        }
        let id = cam.take_id();
        let mut setup = CamSetup {
            id,
            name: format!("Setup {}", cam.setups.len() + 1),
            bodies: Vec::new(),
            stock: Stock::default(),
            wcs: Default::default(),
            work_offset: 1,
            safe_height: CamValue::Number(15.0),
            machine: (cam.machines.len() == 1).then(|| cam.machines[0].id.clone()),
            dialect: None,
            post: Default::default(),
            probe_work_zero: false,
            placement: None,
            visible: true,
            operations: Vec::new(),
        };
        input.apply(&mut setup);
        self.check_cam_setup(&cam, &setup)?;
        cam.setups.push(setup);
        self.commit_cam(cam, "Add CAM Setup", id)
    }

    /// Sets the fields given of a setup (`cam_setup_edit`). Changes of
    /// `visible` alone are its eye ([`Self::set_cam_setup_visible`]).
    pub fn edit_cam_setup(
        &mut self,
        setup: &SetupRef,
        input: CamSetupInput,
    ) -> Result<(), ModelError> {
        if let Some(visible) = input.only_visibility() {
            return self.set_cam_setup_visible(setup, visible);
        }
        let index = self.cam_setup_index(setup)?;
        let mut cam = self.state.cam.clone();
        let mut edited = cam.setups[index].clone();
        input.apply(&mut edited);
        self.check_cam_setup(&cam, &edited)?;
        cam.setups[index] = edited;
        self.commit_cam(cam, "Edit CAM Setup", ())
    }

    /// Shows or hides a setup's toolpaths (its eye in the Browser, kept in
    /// the design): one undo step, `Show Setup 1` or `Hide Setup 1`, as a
    /// body's eye is; none when nothing changes. Not checked otherwise: a
    /// setup whose references are gone still shows and hides, and no
    /// toolpath goes out of date.
    pub fn set_cam_setup_visible(
        &mut self,
        setup: &SetupRef,
        visible: bool,
    ) -> Result<(), ModelError> {
        let index = self.cam_setup_index(setup)?;
        let mut cam = self.state.cam.clone();
        let label = format!(
            "{} {}",
            if visible { "Show" } else { "Hide" },
            cam.setups[index].name
        );
        cam.setups[index].visible = visible;
        self.commit_cam(cam, &label, ())
    }

    /// Deletes a setup with its operations (`cam_setup_delete`).
    pub fn delete_cam_setup(&mut self, setup: &SetupRef) -> Result<(), ModelError> {
        let index = self.cam_setup_index(setup)?;
        let mut cam = self.state.cam.clone();
        cam.setups.remove(index);
        self.commit_cam(cam, "Delete CAM Setup", ())
    }

    /// Moves a setup to a place in the order (`cam_setup_move`; past the
    /// end: last).
    pub fn move_cam_setup(&mut self, setup: &SetupRef, index: usize) -> Result<(), ModelError> {
        let from = self.cam_setup_index(setup)?;
        let mut cam = self.state.cam.clone();
        let moved = cam.setups.remove(from);
        let to = index.min(cam.setups.len());
        cam.setups.insert(to, moved);
        self.commit_cam(cam, "Move CAM Setup", ())
    }

    /// Adds an operation to a setup (`cam_operation_add`; `index` its
    /// place, last when left out). Returns its id.
    pub fn add_cam_operation(
        &mut self,
        setup: &SetupRef,
        input: CamOperationInput,
        index: Option<usize>,
    ) -> Result<u32, ModelError> {
        let s = self.cam_setup_index(setup)?;
        let mut cam = self.state.cam.clone();
        if cam.setups[s].operations.len() >= MAX_OPERATIONS {
            return Err(invalid(format!(
                "A setup has at most {MAX_OPERATIONS} operations."
            )));
        }
        let kind = input.kind.clone().ok_or_else(|| {
            invalid(
                "An operation needs its kind: Facing, Contour, Pocket, Adaptive Clearing, Rest Machining, Drill, 3D Roughing, Raster Finishing or Waterline Finishing.",
            )
        })?;
        let tool = input
            .tool
            .ok_or_else(|| invalid("An operation needs its tool's number."))?;
        let id = cam.take_id();
        let count = cam.setups[s]
            .operations
            .iter()
            .filter(|o| o.kind.id() == kind.id())
            .count()
            + 1;
        let mut op = CamOperation {
            id,
            name: format!("{} {count}", kind.words()),
            kind,
            tool,
            inputs: Vec::new(),
            depths: Default::default(),
            heights: Default::default(),
            cutting: Default::default(),
            stepover: None,
            stock_to_leave: None,
            direction: Default::default(),
            spindle: Default::default(),
            coolant: Default::default(),
            entry: None,
            tabs: None,
            lead: None,
            suppressed: false,
            generated: None,
        };
        input.apply(&mut op);
        self.check_cam_operation(&cam, &cam.setups[s], &op)?;
        let operations = &mut cam.setups[s].operations;
        let at = index.unwrap_or(operations.len()).min(operations.len());
        operations.insert(at, op);
        self.commit_cam(cam, "Add CAM Operation", id)
    }

    /// The setup and index of an operation.
    fn cam_operation_index(&self, operation: u32) -> Result<(usize, usize), ModelError> {
        self.state
            .cam
            .find_operation(operation)
            .ok_or_else(|| invalid("There is no such CAM operation."))
    }

    /// Sets the fields given of an operation (`cam_operation_edit`).
    pub fn edit_cam_operation(
        &mut self,
        operation: u32,
        input: CamOperationInput,
    ) -> Result<(), ModelError> {
        let (s, o) = self.cam_operation_index(operation)?;
        let mut cam = self.state.cam.clone();
        let mut edited = cam.setups[s].operations[o].clone();
        input.apply(&mut edited);
        self.check_cam_operation(&cam, &cam.setups[s], &edited)?;
        cam.setups[s].operations[o] = edited;
        self.commit_cam(cam, "Edit CAM Operation", ())
    }

    /// Deletes an operation (`cam_operation_delete`).
    pub fn delete_cam_operation(&mut self, operation: u32) -> Result<(), ModelError> {
        let (s, o) = self.cam_operation_index(operation)?;
        let mut cam = self.state.cam.clone();
        cam.setups[s].operations.remove(o);
        self.commit_cam(cam, "Delete CAM Operation", ())
    }

    /// Moves an operation to a place in its setup's order, or into another
    /// setup (`cam_operation_move`; past the end: last).
    pub fn move_cam_operation(
        &mut self,
        operation: u32,
        setup: Option<&SetupRef>,
        index: usize,
    ) -> Result<(), ModelError> {
        let (s, o) = self.cam_operation_index(operation)?;
        let target = match setup {
            Some(setup) => self.cam_setup_index(setup)?,
            None => s,
        };
        let mut cam = self.state.cam.clone();
        let moved = cam.setups[s].operations.remove(o);
        if target != s {
            if cam.setups[target].operations.len() >= MAX_OPERATIONS {
                return Err(invalid(format!(
                    "A setup has at most {MAX_OPERATIONS} operations."
                )));
            }
            self.check_cam_operation(&cam, &cam.setups[target], &moved)?;
        }
        let operations = &mut cam.setups[target].operations;
        let to = index.min(operations.len());
        operations.insert(to, moved);
        self.commit_cam(cam, "Move CAM Operation", ())
    }

    /// Puts a machine with its tool library into the design
    /// (`cam_machine_put`), replacing the one of its id.
    pub fn put_cam_machine(&mut self, machine: CamMachine) -> Result<(), ModelError> {
        machine.check().map_err(invalid)?;
        let mut cam = self.state.cam.clone();
        match cam.machines.iter_mut().find(|m| m.id == machine.id) {
            Some(m) => *m = machine,
            None => {
                if cam.machines.len() >= crate::cam::MAX_MACHINES {
                    return Err(invalid(format!(
                        "A design keeps at most {} machines.",
                        crate::cam::MAX_MACHINES
                    )));
                }
                cam.machines.push(machine);
            }
        }
        self.commit_cam(cam, "Set CAM Machine", ())
    }

    /// Takes a machine out of the design (`cam_machine_delete`); refused
    /// while a setup uses it.
    pub fn delete_cam_machine(&mut self, id: &str) -> Result<(), ModelError> {
        let mut cam = self.state.cam.clone();
        let index = cam
            .machines
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| invalid("The design keeps no such machine."))?;
        if cam.setups.iter().any(|s| s.machine.as_deref() == Some(id)) {
            return Err(invalid(
                "A setup uses the machine: choose another machine for it first.",
            ));
        }
        cam.machines.remove(index);
        self.commit_cam(cam, "Delete CAM Machine", ())
    }

    fn cam_machine_index(&self, id: &str) -> Result<usize, ModelError> {
        self.state
            .cam
            .machines
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| invalid("The design keeps no such machine."))
    }

    /// Puts a tool into a machine's tool library (`cam_tool_put`),
    /// replacing the one of its number. Lengths may be given with units.
    /// Returns the tool as kept.
    pub fn put_cam_tool(
        &mut self,
        machine: &str,
        tool: &CamToolInput,
    ) -> Result<crate::cam::CamTool, ModelError> {
        let index = self.cam_machine_index(machine)?;
        let tool = tool.resolve(self.parameters()).map_err(invalid)?;
        let mut cam = self.state.cam.clone();
        let tools = &mut cam.machines[index].tools;
        if tools.get(tool.number).is_none() && tools.tools.len() >= crate::cam::MAX_TOOLS {
            return Err(invalid(format!(
                "A tool library has at most {} tools.",
                crate::cam::MAX_TOOLS
            )));
        }
        tools.put(tool.clone());
        self.commit_cam(cam, "Set CAM Tool", tool)
    }

    /// Takes a tool out of a machine's library (`cam_tool_delete`).
    pub fn delete_cam_tool(&mut self, machine: &str, number: u32) -> Result<(), ModelError> {
        let index = self.cam_machine_index(machine)?;
        let mut cam = self.state.cam.clone();
        if !cam.machines[index].tools.remove(number) {
            return Err(invalid(format!("The machine has no tool {number}.")));
        }
        self.commit_cam(cam, "Delete CAM Tool", ())
    }

    /// Generates the toolpaths of a setup, or of every setup
    /// (`cam_generate`): each operation that computes records the
    /// fingerprint of its inputs (one undo step, `Generate Toolpaths`,
    /// none when every one was up to date). Returns per operation its
    /// state (`up_to_date`, `failed`, `suppressed`), cut length (mm), time
    /// (minutes) and `error`; per setup the machine check's `findings`.
    pub fn generate_cam(&mut self, setup: Option<&SetupRef>) -> Result<Value, ModelError> {
        let indices: Vec<usize> = match setup {
            Some(setup) => vec![self.cam_setup_index(setup)?],
            None => (0..self.state.cam.setups.len()).collect(),
        };
        let mut cam = self.state.cam.clone();
        let mut setups = Vec::new();
        for index in indices {
            let run = self.compute_cam_setup(index, None);
            let rapid = run.machine.map_or(5000.0, |m| m.machine.rapid_feed);
            let mut operations = Vec::new();
            for computed in &run.operations {
                let mut entry = json!({"id": computed.id, "name": computed.name});
                match &computed.result {
                    None => entry["state"] = json!("suppressed"),
                    Some(Ok(op)) => {
                        if let Some(target) = cam.setups[index]
                            .operations
                            .iter_mut()
                            .find(|o| o.id == computed.id)
                        {
                            target.generated = Some(hex(computed.fingerprint));
                        }
                        entry["state"] = json!("up_to_date");
                        entry["cut_length"] = json!(op.toolpath.cut_length());
                        entry["duration"] = json!(op.toolpath.duration(rapid));
                        entry["moves"] = json!(op.toolpath.moves.len());
                    }
                    Some(Err(e)) => {
                        entry["state"] = json!("failed");
                        entry["error"] = json!(e);
                    }
                }
                operations.push(entry);
            }
            let setup_ref = &self.state.cam.setups[index];
            let findings = match Self::cam_program(setup_ref, &run) {
                Ok(program) => Self::cam_findings(setup_ref, &run, &program),
                Err(_) => json!([]),
            };
            let mut value = json!({
                "id": setup_ref.id,
                "name": setup_ref.name,
                "operations": operations,
                "findings": findings,
            });
            if let Err(e) = &run.frame {
                value["error"] = json!(e);
            }
            setups.push(value);
        }
        self.commit_cam(cam, "Generate Toolpaths", json!({"setups": setups}))
    }

    // Queries.

    /// An operation as JSON with its state.
    fn cam_operation_json(op: &CamOperation, fingerprint: u128) -> Value {
        let mut value = serde_json::to_value(op).unwrap_or_else(|_| json!({}));
        value["state"] = json!(state(op, fingerprint));
        value
    }

    /// A setup as JSON: its definition, its work frame (`work`, model
    /// coordinates, or its `error`) and its operations with their states.
    fn cam_setup_json(&self, index: usize) -> Value {
        let setup = &self.state.cam.setups[index];
        let machine = setup
            .machine
            .as_deref()
            .and_then(|m| self.state.cam.machine(m));
        let frame = self.cam_setup_frame(setup);
        let operations: Vec<Value> = setup
            .operations
            .iter()
            .map(|op| {
                Self::cam_operation_json(op, self.cam_fingerprint(setup, &frame, machine, op))
            })
            .collect();
        let mut value = serde_json::to_value(setup).unwrap_or_else(|_| json!({}));
        // Always reported (the file writes it only when off).
        value["visible"] = json!(setup.visible);
        value["operations"] = json!(operations);
        value["work"] = Self::cam_frame_json(&frame);
        let dialect = setup
            .dialect
            .clone()
            .or_else(|| machine.map(|m| m.machine.controller.clone()));
        value["effective_dialect"] = json!(dialect);
        value
    }

    /// The `cam_setups` query: every setup (see [`Self::cam_setup_json`])
    /// and the design's machines (`{id, name, controller, tools}`).
    pub fn cam_setups_json(&self) -> Value {
        let setups: Vec<Value> = (0..self.state.cam.setups.len())
            .map(|i| self.cam_setup_json(i))
            .collect();
        let machines: Vec<Value> = self
            .state
            .cam
            .machines
            .iter()
            .map(|m| {
                json!({"id": m.id, "name": m.machine.name, "controller": m.machine.controller,
                       "tools": m.tools.tools.len()})
            })
            .collect();
        json!({"setups": setups, "machines": machines})
    }

    /// The `cam_operations` query: a setup's operations with their states.
    pub fn cam_operations_json(&self, setup: &SetupRef) -> Result<Value, ModelError> {
        let index = self.cam_setup_index(setup)?;
        let value = self.cam_setup_json(index);
        Ok(json!({"setup": value["id"], "operations": value["operations"]}))
    }

    /// The `cam_toolpaths` query: a setup's toolpaths computed now (all
    /// operations, or `operations`), in work coordinates with the work
    /// frame to place them; per operation its `toolpath` (`moves`),
    /// `tool`, `cut_length`, `duration`, `bounds` or `error`; the machine
    /// check's `findings` when every operation computed.
    pub fn cam_toolpaths_json(
        &self,
        setup: &SetupRef,
        operations: Option<&[u32]>,
    ) -> Result<Value, ModelError> {
        let index = self.cam_setup_index(setup)?;
        let setup = &self.state.cam.setups[index];
        let run = self.compute_cam_setup(index, operations);
        let rapid = run.machine.map_or(5000.0, |m| m.machine.rapid_feed);
        let ops: Vec<Value> = run
            .operations
            .iter()
            .map(|computed| {
                let op = setup.operation(computed.id);
                let mut value = json!({
                    "id": computed.id,
                    "name": computed.name,
                    "state": op.map_or("not_generated", |op| state(op, computed.fingerprint)),
                });
                match &computed.result {
                    None => {}
                    Some(Ok(op)) => {
                        value["toolpath"] = json!(op.toolpath);
                        value["tool"] = json!({"number": op.tool.number, "name": op.tool.name,
                                               "diameter": op.tool.diameter});
                        value["spindle_rpm"] = json!(op.spindle_rpm);
                        value["cut_length"] = json!(op.toolpath.cut_length());
                        value["duration"] = json!(op.toolpath.duration(rapid));
                        value["bounds"] = json!(op.toolpath.bounds());
                    }
                    Some(Err(e)) => value["error"] = json!(e),
                }
                value
            })
            .collect();
        let findings = match Self::cam_program(setup, &run) {
            Ok(program) => Self::cam_findings(setup, &run, &program),
            Err(_) => json!([]),
        };
        Ok(json!({
            "setup": setup.id,
            "work": Self::cam_frame_json(&run.frame),
            "operations": ops,
            "findings": findings,
        }))
    }

    /// The `cam_post` query: a setup's program as the G-code of its
    /// dialect (or `dialect`), in its units (or `units`); `{text,
    /// extension, dialect, label, units}`. Refused while an operation
    /// fails.
    pub fn cam_post_json(
        &self,
        setup: &SetupRef,
        dialect: Option<&str>,
        units: Option<engine::Units>,
    ) -> Result<Value, ModelError> {
        let index = self.cam_setup_index(setup)?;
        let setup = &self.state.cam.setups[index];
        let run = self.compute_cam_setup(index, None);
        let machine = run.machine.ok_or_else(|| invalid(NO_MACHINE))?;
        let id = dialect
            .map(str::to_owned)
            .or_else(|| setup.dialect.clone())
            .unwrap_or_else(|| machine.machine.controller.clone());
        let post = engine::dialect(&id)
            .ok_or_else(|| invalid("MitCad does not write G-code for that controller."))?;
        let program = Self::cam_program(setup, &run).map_err(invalid)?;
        let mut settings = Self::cam_post_settings(setup);
        if let Some(units) = units {
            settings.units = units;
        }
        let text = engine::post(&program, post.as_ref(), &settings, Some(&machine.machine))
            .map_err(|e| invalid(e.message))?;
        Ok(json!({
            "text": text,
            "extension": post.extension(),
            "dialect": post.id(),
            "label": post.label(),
            "units": settings.units,
        }))
    }

    /// The `cam_machines` query: the design's machines with their tool
    /// libraries and the setups that use each.
    pub fn cam_machines_json(&self) -> Value {
        let machines: Vec<Value> = self
            .state
            .cam
            .machines
            .iter()
            .map(|m| {
                let mut value = serde_json::to_value(m).unwrap_or_else(|_| json!({}));
                let used: Vec<u32> = self
                    .state
                    .cam
                    .setups
                    .iter()
                    .filter(|s| s.machine.as_deref() == Some(m.id.as_str()))
                    .map(|s| s.id)
                    .collect();
                value["used_by"] = json!(used);
                value
            })
            .collect();
        json!({"machines": machines})
    }

    // The machine library.

    /// The `cam_library` query: the machines of the user's and the
    /// project's library (`project` the project's folder), and what could
    /// not be read.
    pub fn cam_library_json(&self, project: Option<&Path>) -> Value {
        let mut machines = Vec::new();
        let mut problems = Vec::new();
        let mut folders = Map::new();
        for kind in CamLibrary::ALL {
            let Ok(folder) = library::library_folder(kind, project) else {
                continue;
            };
            folders.insert(kind.id().to_owned(), json!(folder.display().to_string()));
            let (found, trouble) = library::read_library(kind, &folder);
            problems.extend(trouble);
            for m in found {
                let in_design = self
                    .state
                    .cam
                    .machines
                    .iter()
                    .find(|d| {
                        d.origin
                            .as_ref()
                            .is_some_and(|o| o.library == kind.id() && o.id == m.id)
                    })
                    .map(|d| d.id.clone());
                machines.push(json!({
                    "library": kind.id(),
                    "id": m.id,
                    "name": m.machine.name,
                    "controller": m.machine.controller,
                    "version": m.version,
                    "tools": m.tools.tools.len(),
                    "path": m.path.display().to_string(),
                    "in_design": in_design,
                }));
            }
        }
        json!({"machines": machines, "folders": folders, "problems": problems})
    }

    /// Copies a library's machine with its tools into the design
    /// (`cam_machine_from_library`), as `id` there unless `as_id` is
    /// given; replaces the design's machine of that id. Returns the id.
    pub fn copy_cam_machine_from_library(
        &mut self,
        library_kind: CamLibrary,
        id: &str,
        project: Option<&Path>,
        as_id: Option<&str>,
    ) -> Result<String, ModelError> {
        let folder = library::library_folder(library_kind, project).map_err(invalid)?;
        let (found, _) = library::read_library(library_kind, &folder);
        let item = found
            .into_iter()
            .find(|m| m.id == id)
            .ok_or_else(|| invalid("The library has no such machine."))?;
        let mut copy = item.copy();
        if let Some(as_id) = as_id {
            copy.id = as_id.to_owned();
        }
        let kept = copy.id.clone();
        self.put_cam_machine(copy)?;
        Ok(kept)
    }

    /// Saves the design's machine with its tools into the user's or the
    /// project's library (`cam_machine_to_library`) as `as_id` (its own id
    /// when left out), the next version of that file; the design's copy
    /// then names it as its origin (an undo step, `Save CAM Machine`; the
    /// file stays). Returns the file and the version.
    pub fn save_cam_machine_to_library(
        &mut self,
        machine: &str,
        library_kind: CamLibrary,
        project: Option<&Path>,
        as_id: Option<&str>,
    ) -> Result<(PathBuf, u32), ModelError> {
        let index = self.cam_machine_index(machine)?;
        let folder = library::library_folder(library_kind, project).map_err(invalid)?;
        let id = as_id.unwrap_or(machine).to_owned();
        let saved = self.state.cam.machines[index].clone();
        let (path, version) = library::write_machine(&folder, &id, &saved).map_err(invalid)?;
        let mut cam = self.state.cam.clone();
        cam.machines[index].origin = Some(MachineOrigin {
            library: library_kind.id().to_owned(),
            id,
            version,
        });
        self.commit_cam(cam, "Save CAM Machine", (path, version))
    }
}
