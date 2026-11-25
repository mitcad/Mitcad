// SPDX-License-Identifier: MIT
//! Live tools on a lathe with a C axis in the document (mitcad#2067;
//! docs/cam.md, "Live tools on the C axis"): a milling operation in a
//! lathe setup whose lathe has a C axis and live tools cuts with a live
//! tool on the turret, on the part's face or its diameter, and is written
//! by the turning post with the C axis (`mitcad_cam::turning::live`).
//!
//! - **On the face** the operation is computed as on a mill whose work
//!   coordinates are the lathe setup's (the zero on the spindle axis at the
//!   part's face, Z along the spindle away from the chuck), the stock a box
//!   round the bar: its inputs are faces, sketch profiles, edges and points
//!   as a mill's, level with the face. A drilling's bottom is its deepest
//!   hole's when its points are holes' faces.
//! - **On the diameter** the values the lathe's X takes (the top, the
//!   bottom, the clearance and the retract) are diameters. A drilling's
//!   points are holes pointing at the spindle axis (their faces or points
//!   on the diameter), indexed on the bar's cylinder; a contour or a pocket
//!   mills the faces it is given round the spindle axis (the floors of a
//!   wrapped slot or pocket, all at one radius), their boundaries unrolled
//!   on that cylinder from the widest gap round the axis.
//! - **On the sub-spindle** (`sub_spindle`, mitcad#2375) every place is
//!   computed the same way in the sub-spindle's work coordinates (the
//!   setup's turned round about its X at the part's back,
//!   `Document::turn_sub_frame`); the view, the simulation and the checks
//!   turn its places back to where the part was.

use std::f64::consts::{PI, TAU};

use serde_json::{Value, json};

use super::super::Document;
use super::turning::TurnFrame;
use super::{Outlines, SetupFrame, level_loops};
use crate::cam::engine::ProgramOp;
use crate::cam::engine::stock::{self as sim, HeightMap, MAX_NODES};
use crate::cam::engine::turning::live::PlaneKind;
use crate::cam::engine::turning::{
    self as turn, LiveOp, LiveSide, TurnPath, TurnSpindle, TurningProgram, TurningProgramOp,
    TurningTool, TurningToolKind,
};
use crate::cam::engine::{Move, Toolpath, TriMesh};
use crate::cam::outline::{self, Seg};
use crate::cam::{
    CamInput, CamMachine, CamOperation, CamSetup, CamValue, LiveMount, LivePlace, OperationKind,
    ValueKind,
};
use crate::features::geom_ref::GeomRef;
use crate::kernel::Kernel;
use mitcad_vec3::{dot, norm, scale};

type P3 = [f64; 3];

/// A hole a live tool drills: where its axis meets the spindle's plane or
/// its picked point (work coordinates), its axis when the pick has one,
/// and the points of its face's boundary when it is a face.
struct Hole {
    at: P3,
    axis: Option<(P3, P3)>,
    boundary: Vec<P3>,
}

impl<K: Kernel> Document<K> {
    /// The face's work coordinates of a lathe setup as a mill's: its
    /// frame, the stock a box round the bar from its back to its face.
    pub(super) fn live_face_frame(
        &self,
        setup: &CamSetup,
        tf: &TurnFrame,
    ) -> Result<SetupFrame, String> {
        let r = tf.part.stock_radius;
        let safe = setup
            .safe_height
            .evaluate(self.parameters(), ValueKind::Length)?;
        Ok(SetupFrame {
            frame: tf.frame,
            stock_min: [-r, -r, tf.stock_back],
            stock_max: [r, r, tf.part.stock_face],
            bodies_top: tf.part.face_z(),
            safe_z: tf.part.stock_face + safe,
        })
    }

    /// The points of a face's boundary in work coordinates.
    fn live_face_points(
        &self,
        tf: &TurnFrame,
        input: &CamInput,
        body: crate::ids::BodyUid,
        face: &crate::topo::FaceName,
    ) -> Result<Vec<Vec<P3>>, String> {
        let shape = self.body_shape(body).ok_or_else(|| {
            "A body the operation reads is not in the design at the timeline marker.".to_owned()
        })?;
        let placement = self.cam_input_placement(input)?;
        let curves = self
            .kernel()
            .face_boundary(shape, face)
            .map_err(|_| "A face of the operation cannot be read.".to_owned())?;
        let mut out = Vec::new();
        for (_, curve) in curves {
            if let Some(piece) = outline::piece_of_curve(&curve) {
                let points: Vec<P3> = outline::piece_points(&piece.placed(&placement))
                    .into_iter()
                    .map(|p| tf.frame.to_work(p))
                    .collect();
                out.push(points);
            }
        }
        if out.is_empty() {
            return Err("A face of the operation is gone.".to_owned());
        }
        Ok(out)
    }

    /// A drilling's hole from a point input.
    fn live_hole(&self, tf: &TurnFrame, input: &CamInput) -> Result<Hole, String> {
        let CamInput::Point { at, occurrence } = input else {
            return Err("Pick the holes: points, round edges or holes' faces.".to_owned());
        };
        let placement = self.cam_placement(self.cam_ref_owner(at), occurrence)?;
        let point = tf.frame.to_work(self.cam_point(at, occurrence)?);
        let axis = self.resolve_axis(at).ok().and_then(|a| {
            let d = tf.frame.direction(placement.apply_vector(a.direction));
            let n = norm(d);
            (n > 1e-12).then(|| {
                (
                    tf.frame.to_work(placement.apply_point(a.origin)),
                    scale(d, 1.0 / n),
                )
            })
        });
        let boundary = match at {
            GeomRef::Face { body, face } => {
                let face_input = CamInput::Face {
                    body: *body,
                    face: face.clone(),
                    occurrence: occurrence.clone(),
                };
                self.live_face_points(tf, &face_input, *body, face)?
                    .into_iter()
                    .flatten()
                    .collect()
            }
            _ => Vec::new(),
        };
        Ok(Hole {
            at: point,
            axis,
            boundary,
        })
    }

    /// A value of the lathe's X given as a diameter, as a radius.
    fn live_radius(&self, value: &Option<CamValue>) -> Result<Option<CamValue>, String> {
        Ok(self
            .cam_value(value, ValueKind::Length)?
            .map(|d| CamValue::Number(d / 2.0)))
    }

    /// The operation with its values along the lathe's X (diameters) as
    /// radii, for the milling engine on the diameter.
    fn live_on_radii(&self, op: &CamOperation) -> Result<CamOperation, String> {
        let mut op = op.clone();
        op.depths.top = self.live_radius(&op.depths.top)?;
        op.depths.bottom = self.live_radius(&op.depths.bottom)?;
        op.heights.clearance = self.live_radius(&op.heights.clearance)?;
        op.heights.retract = self.live_radius(&op.heights.retract)?;
        Ok(op)
    }

    /// The faces of a milling on the diameter unrolled on their cylinder
    /// about the spindle axis: the outlines (the length round it, work Z,
    /// at the cylinder's radius) and the radius.
    fn live_unrolled(&self, tf: &TurnFrame, op: &CamOperation) -> Result<(Outlines, f64), String> {
        let mut lines: Vec<Vec<P3>> = Vec::new();
        for input in &op.inputs {
            let CamInput::Face { body, face, .. } = input else {
                return Err(
                    "On the diameter a live tool mills the faces round the spindle axis it is given."
                        .to_owned(),
                );
            };
            lines.extend(self.live_face_points(tf, input, *body, face)?);
        }
        let points: Vec<P3> = lines.iter().flatten().copied().collect();
        if points.is_empty() {
            return Err(
                "The operation has no faces to mill on the diameter: pick the floors round the spindle axis."
                    .to_owned(),
            );
        }
        let radius = points.iter().map(|p| p[0].hypot(p[1])).sum::<f64>() / points.len() as f64;
        if radius < 1e-6
            || points
                .iter()
                .any(|p| (p[0].hypot(p[1]) - radius).abs() > 0.01)
        {
            return Err(
                "A face of the operation is not round the spindle axis at one radius: pick the floors of what it mills on the diameter."
                    .to_owned(),
            );
        }
        // The seam: in the middle of the widest gap round the axis.
        let mut angles: Vec<f64> = points.iter().map(|p| p[1].atan2(p[0])).collect();
        angles.sort_by(f64::total_cmp);
        let mut gap = (
            angles[0] + TAU - angles[angles.len() - 1],
            angles[angles.len() - 1],
        );
        for w in angles.windows(2) {
            if w[1] - w[0] > gap.0 {
                gap = (w[1] - w[0], w[0]);
            }
        }
        if gap.0 < 1.0_f64.to_radians() {
            return Err(
                "A face of the operation goes all round the part: turn it (grooving) instead."
                    .to_owned(),
            );
        }
        let seam = gap.1 + gap.0 / 2.0;
        // The faces' middle angle within a half turn of zero.
        let shift = ((seam + PI + PI) / TAU).floor() * TAU;
        let unroll = |p: P3| -> P3 {
            let a = seam + (p[1].atan2(p[0]) - seam).rem_euclid(TAU) - shift;
            [a * radius, p[2], radius]
        };
        let mut segs = Vec::new();
        for line in &lines {
            for w in line.windows(2) {
                segs.push(Seg {
                    a: unroll(w[0]),
                    b: unroll(w[1]),
                    bulge: 0.0,
                });
            }
        }
        let (loops, z) = level_loops(
            outline::chains(segs),
            "A face of the operation is not round the spindle axis at one radius.",
        )?;
        let mut out = Outlines::default();
        out.add_loops(loops, z);
        Ok((out, radius))
    }

    /// Sketch geometry wrapped onto the diameter (mitcad#2319): the
    /// sketch's plane runs along the spindle axis, and its lengths across
    /// the axis become lengths round the cylinder it wraps on (the plane's
    /// distance from the axis, or the bar's radius for a plane through the
    /// axis), unrolled as [`Self::live_unrolled`] unrolls faces: x the
    /// length round from the work X towards the work Y, y the work Z. Arcs
    /// stay arcs. Gives the outlines (at the cylinder's radius) and the
    /// radius.
    fn live_wrapped(&self, tf: &TurnFrame, op: &CamOperation) -> Result<(Outlines, f64), String> {
        // The pieces in model space, each input's own.
        let mut pieces: Vec<(outline::Piece, bool)> = Vec::new();
        for input in &op.inputs {
            let placement = self.cam_input_placement(input)?;
            match input {
                CamInput::Region { sketch, region, .. } => {
                    let (frame, profile) = self
                        .profile_region(*sketch, region)
                        .ok_or_else(|| "A sketch profile of the operation is gone.".to_owned())?;
                    for profile_loop in &profile.loops {
                        for segment in &profile_loop.segments {
                            let piece = outline::piece_of_segment(&segment.geometry, &frame)?
                                .placed(&placement);
                            pieces.push((piece, true));
                        }
                    }
                }
                CamInput::Curves { sketch, curves, .. } => {
                    let output = self
                        .sketch_output(*sketch)
                        .ok_or_else(|| "A sketch of the operation is gone or failed.".to_owned())?;
                    for name in curves {
                        let uid = crate::ids::EntityUid::parse_curve(name)
                            .map_err(|_| "A sketch curve is named like c3.".to_owned())?;
                        let curve =
                            output.solved.curves.get(&uid).ok_or_else(|| {
                                "A sketch curve of the operation is gone.".to_owned()
                            })?;
                        let piece =
                            outline::piece_of_sketch_curve(curve, &output.frame).placed(&placement);
                        pieces.push((piece, false));
                    }
                }
                _ => {
                    return Err(
                        "Wrap sketch profiles or curves onto the diameter, or pick the faces round the spindle axis: not both."
                            .to_owned(),
                    );
                }
            }
        }
        // The sketch's plane: its trace across the axis (the points' main
        // direction in the work XY), its normal away from the axis.
        let points: Vec<P3> = pieces
            .iter()
            .flat_map(|(p, _)| outline::piece_points(p))
            .map(|p| tf.frame.to_work(p))
            .collect();
        if points.is_empty() {
            return Err("The operation has nothing to wrap onto the diameter.".to_owned());
        }
        let n = points.len() as f64;
        let (mx, my) = (
            points.iter().map(|p| p[0]).sum::<f64>() / n,
            points.iter().map(|p| p[1]).sum::<f64>() / n,
        );
        let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
        for p in &points {
            let (dx, dy) = (p[0] - mx, p[1] - my);
            sxx += dx * dx;
            syy += dy * dy;
            sxy += dx * dy;
        }
        let along = 0.5 * (2.0 * sxy).atan2(sxx - syy);
        let trace = [along.cos(), along.sin(), 0.0];
        let mut normal = [-trace[1], trace[0], 0.0];
        if normal[0] * mx + normal[1] * my < 0.0 {
            normal = scale(normal, -1.0);
        }
        let distance = normal[0] * mx + normal[1] * my;
        if points
            .iter()
            .any(|p| (normal[0] * p[0] + normal[1] * p[1] - distance).abs() > 0.01)
        {
            return Err(
                "A sketch wrapped onto the diameter must lie on a plane along the spindle axis."
                    .to_owned(),
            );
        }
        let radius = if distance > 1e-6 {
            distance
        } else {
            tf.part.stock_radius
        };
        // The wrapping frame: x round the axis at the plane's foot, y along
        // the spindle, z away from the axis.
        let round = [-normal[1], normal[0], 0.0];
        let model = |w: P3| {
            let f = &tf.frame;
            std::array::from_fn(|k| f.x[k] * w[0] + f.y[k] * w[1] + f.z[k] * w[2])
        };
        let wrap = outline::WorkFrame {
            origin: tf.frame.origin,
            x: model(round),
            y: tf.frame.z,
            z: model(normal),
        };
        let start = normal[1].atan2(normal[0]) * radius;
        let mut loops_segs = Vec::new();
        let mut open_segs = Vec::new();
        for (piece, closed) in &pieces {
            for s in outline::work_segments(piece, &wrap) {
                let s = Seg {
                    a: [s.a[0] + start, s.a[1], radius],
                    b: [s.b[0] + start, s.b[1], radius],
                    bulge: s.bulge,
                };
                if *closed {
                    loops_segs.push(s);
                } else {
                    open_segs.push(s);
                }
            }
        }
        let mut out = Outlines::default();
        if !loops_segs.is_empty() {
            let (loops, z) = level_loops(
                outline::chains(loops_segs),
                "A sketch wrapped onto the diameter must lie on a plane along the spindle axis.",
            )?;
            out.add_loops(loops, z);
        }
        for chain in outline::chains(open_segs) {
            out.add_chain(chain);
        }
        Ok((out, radius))
    }

    /// The angles of a live tool's plane (mitcad#2319): the operation's,
    /// else from its first input: a planar face's normal, a hole's axis,
    /// each turned away from the part's middle on the spindle axis.
    fn live_plane_angles(&self, tf: &TurnFrame, op: &CamOperation) -> Result<(f64, f64), String> {
        let given = |v: Option<&CamValue>| -> Result<Option<f64>, String> {
            v.map(|v| v.evaluate(self.parameters(), ValueKind::Angle))
                .transpose()
        };
        let plane = op.plane.clone().unwrap_or_default();
        let (c, b) = (given(plane.c.as_ref())?, given(plane.b.as_ref())?);
        if let (Some(c), Some(b)) = (c, b) {
            return Ok((c, b));
        }
        let middle = [0.0, 0.0, 0.5 * (tf.part.face_z() + tf.part.back_z())];
        let found = op.inputs.iter().find_map(|input| match input {
            CamInput::Face {
                body,
                face,
                occurrence,
            } => {
                let reference = GeomRef::Face {
                    body: *body,
                    face: face.clone(),
                };
                let (normal, _) = self.cam_direction(&reference, occurrence, true)?;
                let at = self.live_face_points(tf, input, *body, face).ok()?;
                let at: Vec<P3> = at.into_iter().flatten().collect();
                let k = at.len().max(1) as f64;
                let centre: P3 = std::array::from_fn(|i| at.iter().map(|p| p[i]).sum::<f64>() / k);
                Some((tf.frame.direction(normal), centre))
            }
            CamInput::Point { .. } => {
                let hole = self.live_hole(tf, input).ok()?;
                hole.axis.map(|(_, d)| (d, hole.at))
            }
            _ => None,
        });
        let Some((mut n, at)) = found else {
            return Err(
                "Give the plane's angles, or pick a planar face or a hole the plane is square to."
                    .to_owned(),
            );
        };
        let length = norm(n);
        if length < 1e-12 {
            return Err("The plane's direction cannot be read from what is picked.".to_owned());
        }
        n = scale(n, 1.0 / length);
        let away: P3 = std::array::from_fn(|i| at[i] - middle[i]);
        if dot(n, away) < 0.0 {
            n = scale(n, -1.0);
        }
        let b_found = n[2].clamp(-1.0, 1.0).acos().to_degrees();
        let c_found = if n[0].hypot(n[1]) > 1e-9 {
            n[1].atan2(n[0]).to_degrees()
        } else {
            0.0
        };
        Ok((c.unwrap_or(c_found), b.unwrap_or(b_found)))
    }

    /// A live tool's plane as a mill's setup frame (mitcad#2319): the
    /// plane's frame in the model, the stock the bar's box seen in it.
    fn live_plane_frame(
        &self,
        setup: &CamSetup,
        tf: &TurnFrame,
        side: LiveSide,
    ) -> Result<SetupFrame, String> {
        let safe = setup
            .safe_height
            .evaluate(self.parameters(), ValueKind::Length)?;
        let model = |w: P3| -> P3 {
            let f = &tf.frame;
            std::array::from_fn(|k| f.x[k] * w[0] + f.y[k] * w[1] + f.z[k] * w[2])
        };
        let frame = outline::WorkFrame {
            origin: tf.frame.origin,
            x: model(side.to_work([1.0, 0.0, 0.0])),
            y: model(side.to_work([0.0, 1.0, 0.0])),
            z: model(side.to_work([0.0, 0.0, 1.0])),
        };
        let r = tf.part.stock_radius;
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for x in [-r, r] {
            for y in [-r, r] {
                for z in [tf.stock_back, tf.part.stock_face] {
                    let q = side.from_work([x, y, z]);
                    for k in 0..3 {
                        lo[k] = lo[k].min(q[k]);
                        hi[k] = hi[k].max(q[k]);
                    }
                }
            }
        }
        Ok(SetupFrame {
            frame,
            stock_min: lo,
            stock_max: hi,
            bodies_top: hi[2],
            safe_z: hi[2] + safe,
        })
    }

    /// A live tool's operation as the turning program has it.
    fn live_program_op(
        machine: &CamMachine,
        op: &CamOperation,
        side: LiveSide,
        milled: ProgramOp,
    ) -> TurningProgramOp {
        let head = machine
            .tools
            .get(op.tool)
            .is_some_and(|t| t.mount == Some(LiveMount::Head));
        let m = &machine.machine;
        TurningProgramOp {
            name: op.name.clone(),
            tool: TurningTool {
                number: milled.tool.number,
                offset: 0,
                name: milled.tool.name.clone(),
                kind: TurningToolKind::Live,
                nose_radius: 0.0,
                max_depth: milled.tool.flute_length.max(1e-3),
            },
            spindle: TurnSpindle::Rpm {
                rpm: milled.spindle_rpm,
            },
            direction: milled.spindle,
            coolant: milled.coolant,
            path: TurnPath::default(),
            live: Some(LiveOp {
                side,
                cutter: milled.tool,
                path: milled.toolpath,
                rapid_feed: m.max_feed.min(m.rapid_feed).max(1.0),
                head,
            }),
            sub: false,
        }
    }

    /// One live tool operation of a lathe setup (the module's
    /// documentation).
    pub(super) fn live_compute_operation(
        &self,
        setup: &CamSetup,
        tf: &TurnFrame,
        machine: &CamMachine,
        op: &CamOperation,
    ) -> Result<TurningProgramOp, String> {
        if !machine.machine.lathe.as_ref().is_some_and(|l| l.mills()) {
            return Err("A lathe setup takes turning operations only.".to_owned());
        }
        if !op.kind.cuts_live() {
            return Err(
                "On a lathe, live tools face, contour, pocket, clear adaptively and drill."
                    .to_owned(),
            );
        }
        let tool = machine.tools.get(op.tool).ok_or_else(|| {
            "The operation's tool is not in the machine's tool library.".to_owned()
        })?;
        let place = op.live.unwrap_or_default();
        let mount = tool.mount.unwrap_or_default();
        match (place, mount) {
            (LivePlace::Face, LiveMount::Radial) => {
                return Err("Its tool sits in a radial holder: it cuts the diameter.".to_owned());
            }
            (LivePlace::Diameter, LiveMount::Axial) => {
                return Err("Its tool sits in an axial holder: it cuts the face.".to_owned());
            }
            _ => {}
        }
        let drill = matches!(op.kind, OperationKind::Drill { .. });
        let safe = setup
            .safe_height
            .evaluate(self.parameters(), ValueKind::Length)?;
        // On a plane with the Y axis, the B head tilting the tool where the
        // plane is neither the face nor along the axis (mitcad#2319).
        if place == LivePlace::Plane {
            let (c, b) = self.live_plane_angles(tf, op)?;
            let side = LiveSide::Plane { c, b };
            match (side.plane_kind(), mount) {
                (Some(PlaneKind::Tilted), LiveMount::Axial | LiveMount::Radial) => {
                    return Err("The plane is tilted: its tool must sit in the B head.".to_owned());
                }
                (Some(PlaneKind::Face), LiveMount::Radial) => {
                    return Err(
                        "Its tool sits in a radial holder: it cuts planes along the spindle axis."
                            .to_owned(),
                    );
                }
                (Some(PlaneKind::Side), LiveMount::Axial) => {
                    return Err(
                        "Its tool sits in an axial holder: it cuts planes square to the spindle axis."
                            .to_owned(),
                    );
                }
                _ => {}
            }
            let sf = self.live_plane_frame(setup, tf, side)?;
            let mut op = op.clone();
            if drill {
                // Holes square to the plane; the deepest hole's bottom.
                let tool_axis = side.tool_axis([0.0; 3]);
                let mut deepest: Option<f64> = None;
                for input in &op.inputs {
                    let hole = self.live_hole(tf, input)?;
                    if let Some((_, d)) = hole.axis
                        && dot(d, tool_axis).abs() < 1.0 - 1e-6
                    {
                        return Err(
                            "A hole of the drilling is not square to its plane: give the plane the hole's angles."
                                .to_owned(),
                        );
                    }
                    for p in hole.boundary {
                        let depth = side.from_work(p)[2];
                        deepest = Some(deepest.map_or(depth, |d: f64| d.min(depth)));
                    }
                }
                if op.depths.bottom.is_none() {
                    let bottom = deepest.ok_or_else(|| {
                        "Set the drilling's bottom: its holes are points, not holes' faces."
                            .to_owned()
                    })?;
                    op.depths.bottom = Some(CamValue::Number(bottom));
                }
            }
            let milled = self.cam_compute_with(setup, &sf, machine, &op, None)?;
            return Ok(Self::live_program_op(machine, &op, side, milled));
        }
        match place {
            LivePlace::Plane => unreachable!("a plane is computed above"),
            LivePlace::Face => {
                let sf = self.live_face_frame(setup, tf)?;
                let mut op = op.clone();
                if drill {
                    // Holes along the spindle; the deepest hole's bottom.
                    let mut deepest: Option<f64> = None;
                    for input in &op.inputs {
                        let hole = self.live_hole(tf, input)?;
                        if let Some((_, d)) = hole.axis
                            && d[2].abs() < 1.0 - 1e-6
                        {
                            return Err(
                                "A hole of the drilling does not run along the spindle axis: drill it on the diameter."
                                    .to_owned(),
                            );
                        }
                        for p in hole.boundary {
                            deepest = Some(deepest.map_or(p[2], |d: f64| d.min(p[2])));
                        }
                    }
                    if op.depths.bottom.is_none() {
                        let bottom = deepest.ok_or_else(|| {
                            "Set the drilling's bottom: its holes are points, not holes' faces."
                                .to_owned()
                        })?;
                        op.depths.bottom = Some(CamValue::Number(bottom));
                    }
                }
                let milled = self.cam_compute_with(setup, &sf, machine, &op, None)?;
                Ok(Self::live_program_op(machine, &op, LiveSide::Face, milled))
            }
            LivePlace::Diameter => {
                let mut op = self.live_on_radii(op)?;
                let stock = tf.part.stock_radius;
                let (outlines, radius) = if drill {
                    let mut out = Outlines::default();
                    let mut deepest: Option<f64> = None;
                    for input in &op.inputs {
                        let hole = self.live_hole(tf, input)?;
                        let (angle, z) = match hole.axis {
                            Some((o, d)) => {
                                // Across the spindle and through its axis.
                                let across = o[0] * d[1] - o[1] * d[0];
                                if d[2].abs() > 1e-6 || across.abs() > 0.01 {
                                    return Err(
                                        "A hole of the drilling does not point at the spindle axis: drill it on the face, or with a Y axis."
                                            .to_owned(),
                                    );
                                }
                                let angle = if o[0].hypot(o[1]) > 1e-6 {
                                    o[1].atan2(o[0])
                                } else {
                                    d[1].atan2(d[0])
                                };
                                (angle, o[2])
                            }
                            None => {
                                let p = hole.at;
                                if p[0].hypot(p[1]) < 1e-6 {
                                    return Err(
                                        "A hole on the diameter needs a point off the spindle axis."
                                            .to_owned(),
                                    );
                                }
                                (p[1].atan2(p[0]), p[2])
                            }
                        };
                        let toward = [angle.cos(), angle.sin(), 0.0];
                        for p in &hole.boundary {
                            let rho = dot(*p, toward);
                            deepest = Some(deepest.map_or(rho, |d: f64| d.min(rho)));
                        }
                        out.points.push([angle * stock, z]);
                    }
                    if op.depths.bottom.is_none() {
                        let bottom = deepest.ok_or_else(|| {
                            "Set the drilling's bottom diameter: its holes are points, not holes' faces."
                                .to_owned()
                        })?;
                        op.depths.bottom = Some(CamValue::Number(bottom));
                    }
                    (out, stock)
                } else if op
                    .inputs
                    .iter()
                    .any(|i| matches!(i, CamInput::Region { .. } | CamInput::Curves { .. }))
                {
                    // Sketch geometry wrapped onto the diameter
                    // (mitcad#2319): its bottom a diameter of its own.
                    if op.depths.bottom.is_none() {
                        return Err(
                            "Set the bottom diameter of what is wrapped onto the diameter."
                                .to_owned(),
                        );
                    }
                    self.live_wrapped(tf, &op)?
                } else {
                    self.live_unrolled(tf, &op)?
                };
                let wrapped = op
                    .inputs
                    .iter()
                    .any(|i| matches!(i, CamInput::Region { .. } | CamInput::Curves { .. }));
                let lo = outlines
                    .points
                    .iter()
                    .map(|p| p[0])
                    .chain(outlines.extent.map(|(lo, _)| lo[0]))
                    .fold(f64::INFINITY, f64::min);
                let hi = outlines
                    .points
                    .iter()
                    .map(|p| p[0])
                    .chain(outlines.extent.map(|(_, hi)| hi[0]))
                    .fold(f64::NEG_INFINITY, f64::max);
                let (lo, hi) = if lo.is_finite() { (lo, hi) } else { (0.0, 0.0) };
                let floor = if drill || wrapped {
                    self.cam_value(&op.depths.bottom, ValueKind::Length)?
                        .unwrap_or(0.0)
                } else {
                    radius
                };
                let sf = SetupFrame {
                    frame: tf.frame,
                    stock_min: [lo, tf.stock_back, floor.min(stock)],
                    stock_max: [hi.max(lo + 1e-3), tf.part.stock_face, stock],
                    bodies_top: stock,
                    safe_z: stock + safe,
                };
                let milled = self.cam_compute_with(setup, &sf, machine, &op, Some(outlines))?;
                Ok(Self::live_program_op(
                    machine,
                    &op,
                    LiveSide::Diameter { radius },
                    milled,
                ))
            }
        }
    }

    /// A live tool's toolpath for the view: its moves in the lathe setup's
    /// work coordinates, arcs and the unrolled moves as short lines.
    pub(super) fn live_view_path(live: &LiveOp) -> Toolpath {
        let mut moves = Vec::new();
        let mut at: Option<P3> = None;
        for m in &live.path.moves {
            let pts = match at {
                Some(from) => {
                    let mut pts = crate::cam::engine::toolpath::move_points(from, m, 0.005);
                    // A line round the cylinder is an arc in the part.
                    if matches!(live.side, LiveSide::Diameter { .. }) {
                        let to = m.to();
                        let steps = ((to[0] - from[0]).abs() / 0.5).ceil().max(1.0) as usize;
                        if !matches!(m, Move::Arc { .. }) && steps > 1 {
                            pts = (1..=steps)
                                .map(|k| {
                                    let t = k as f64 / steps as f64;
                                    std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t)
                                })
                                .collect();
                        }
                    }
                    pts
                }
                None => vec![m.to()],
            };
            for p in pts {
                let to = live.side.to_work(p);
                moves.push(match *m {
                    Move::Rapid { .. } => Move::Rapid { to },
                    Move::Line { feed, .. } | Move::Arc { feed, .. } => Move::Line { to, feed },
                });
            }
            at = Some(m.to());
        }
        Toolpath {
            moves,
            cycles: Vec::new(),
        }
    }

    /// The live tools' cuts of a lathe setup's program for `cam_simulate`
    /// (`ids`: each program operation's id): per side (the face, each
    /// cylinder) the map of the stock before and after them, the part's
    /// map from its bodies, the material left over the part and the rapids
    /// that cut; the turned stock on nodes from `z_min`, `step` apart.
    pub(super) fn live_simulate_json(
        &self,
        setup: &CamSetup,
        tf: &TurnFrame,
        program: &TurningProgram,
        ids: &[u32],
        (z_min, step): (f64, f64),
        tolerance: f64,
    ) -> Result<Value, String> {
        let finest = program
            .operations
            .iter()
            .filter_map(|o| o.live.as_ref())
            .map(|l| l.cutter.diameter / 8.0)
            .fold(f64::INFINITY, f64::min);
        if !finest.is_finite() {
            return Ok(json!([]));
        }
        let sims = turn::simulate_live(&tf.part, program, z_min, step, finest, MAX_NODES);
        let sf = self.live_face_frame(setup, tf)?;
        let bodies = self.cam_setup_bodies(setup)?;
        let mut out = Vec::new();
        // The sub-spindle's work coordinates of a point in the setup's, and
        // back (both the part turned round about X at its back,
        // mitcad#2375).
        let back = tf.part.back_z();
        let turn_round = |p: P3| -> P3 { [p[0], -p[1], back - p[2]] };
        for s in &sims {
            let grid = s.initial.grid;
            let deviation = (grid.step[0].min(grid.step[1]) / 4.0).clamp(0.001, 0.05);
            let mut mesh = self.cam_work_mesh(&bodies, &sf, deviation)?;
            if s.sub {
                for v in &mut mesh.vertices {
                    *v = turn_round(*v);
                }
            }
            let sub = s.sub;
            let model = |w: P3| tf.frame.to_model(if sub { turn_round(w) } else { w });
            let part = match s.side {
                LiveSide::Face => HeightMap::top_of_mesh(grid, s.initial.bottom, &mesh),
                LiveSide::Diameter { radius } => {
                    let middle = grid.x(grid.nx / 2);
                    HeightMap::top_of_mesh(grid, 0.0, &unrolled_mesh(&mesh, radius, middle))
                }
                // The bodies seen along a plane's tool (mitcad#2319).
                LiveSide::Plane { .. } => {
                    let seen = TriMesh {
                        vertices: mesh.vertices.iter().map(|p| s.side.from_work(*p)).collect(),
                        triangles: mesh.triangles.clone(),
                    };
                    HeightMap::top_of_mesh(grid, s.initial.bottom, &seen)
                }
            };
            let spacing = s
                .operations
                .iter()
                .filter_map(|&i| program.operations[i].live.as_ref())
                .map(|l| l.cutter.diameter)
                .fold(f64::INFINITY, f64::min);
            let left = sim::remaining(&s.simulation.stock, &part, tolerance, spacing);
            let id = |i: usize| ids.get(i).copied().unwrap_or_default();
            let side = s.side;
            let place = |p: P3| json!({"at": p, "model": model(side.to_work(p))});
            let rapid_cuts: Vec<Value> = s
                .simulation
                .rapid_cuts
                .iter()
                .map(|c| {
                    json!({
                        "message": c.message,
                        "operation": id(c.operation),
                        "move": c.move_index,
                        "depth": c.depth,
                        "at": c.at,
                        "model": model(side.to_work(c.at)),
                    })
                })
                .collect();
            let thick = |found: &[sim::Thickness]| -> Vec<Value> {
                found
                    .iter()
                    .map(|t| {
                        let mut v = place(t.at);
                        v["thickness"] = json!(t.thickness);
                        v
                    })
                    .collect()
            };
            out.push(json!({
                "side": side,
                "sub_spindle": sub,
                "operations": s.operations.iter().map(|&i| id(i)).collect::<Vec<u32>>(),
                "grid": {
                    "origin": grid.origin,
                    "step": grid.step,
                    "nx": grid.nx,
                    "ny": grid.ny,
                    "bottom": s.initial.bottom,
                    "top": s.initial.top(),
                },
                "initial": s.initial.runs(),
                "heights": s.simulation.stock.runs(),
                "part": part.runs(),
                "removed": s.simulation.removed,
                "rapid_cuts": rapid_cuts,
                "remaining": {
                    "tolerance": left.tolerance,
                    "max": left.max,
                    "mean": left.mean,
                    "volume": left.volume,
                    "thickest": thick(&left.thickest),
                    "gouge": left.gouge,
                    "gouges": thick(&left.gouges),
                },
            }));
        }
        Ok(json!(out))
    }
}

/// The most cubes of the 3D stock (mitcad#2319): a bar 40 mm across and
/// 60 mm long in cubes of about 0.6 mm.
const SOLID_CUBES: usize = 400_000;

impl<K: Kernel> Document<K> {
    /// The lathe setup's program cut out of the bar in 3D for
    /// `cam_simulate` (mitcad#2319, `turn::solid`): the stock's surface as
    /// a mesh in model coordinates, its volumes (mm³: the bar's, what is
    /// left, what the live tools took, the material over the part and the
    /// part's cut away) and the rapids that cut (`ids`: each program
    /// operation's id); the turning on nodes from `z_min`, `step` apart.
    pub(super) fn solid_simulate_json(
        &self,
        setup: &CamSetup,
        tf: &TurnFrame,
        program: &TurningProgram,
        ids: &[u32],
        (z_min, step): (f64, f64),
    ) -> Result<Value, String> {
        // Cubes a sixth of the smallest live tool, none under 0.2 mm.
        let finest = program
            .operations
            .iter()
            .filter_map(|o| o.live.as_ref())
            .map(|l| l.cutter.diameter / 6.0)
            .fold(f64::INFINITY, f64::min);
        let finest = if finest.is_finite() {
            finest.max(0.2)
        } else {
            0.5
        };
        let sf = self.live_face_frame(setup, tf)?;
        let bodies = self.cam_setup_bodies(setup)?;
        let mesh = self.cam_work_mesh(&bodies, &sf, 0.05)?;
        let sim = turn::simulate_solid(
            &tf.part,
            program,
            z_min,
            step,
            finest,
            SOLID_CUBES,
            Some(&mesh),
        );
        let surface = sim.voxels.mesh();
        let vertices: Vec<f64> = surface
            .vertices
            .iter()
            .flat_map(|p| tf.frame.to_model(*p))
            .collect();
        let triangles: Vec<u32> = surface.triangles.iter().flatten().copied().collect();
        let id = |i: usize| ids.get(i).copied().unwrap_or_default();
        let rapid_cuts: Vec<Value> = sim
            .rapid_cuts
            .iter()
            .map(|c| {
                json!({
                    "message": c.message,
                    "operation": id(c.operation),
                    "at": c.at,
                    "model": c.at.map(|p| tf.frame.to_model(p)),
                })
            })
            .collect();
        let gouges: Vec<Value> = sim
            .gouges
            .iter()
            .map(|p| json!({"at": p, "model": tf.frame.to_model(*p)}))
            .collect();
        Ok(json!({
            "step": sim.voxels.step,
            "cubes": sim.voxels.n,
            "initial": sim.initial,
            "volume": sim.voxels.volume(),
            "milled": sim.milled,
            "left": sim.left,
            "gouge": sim.gouge,
            "gouges": gouges,
            "rapid_cuts": rapid_cuts,
            "mesh": {"vertices": vertices, "triangles": triangles},
        }))
    }
}

/// A mesh in work coordinates unrolled on the cylinder of `radius` about
/// the spindle axis: each vertex at its length round it (within half a
/// turn of `middle`), its work Z and its distance from the axis; the
/// triangles across the seam left out.
fn unrolled_mesh(mesh: &TriMesh, radius: f64, middle: f64) -> TriMesh {
    let centre = middle / radius;
    let vertices: Vec<P3> = mesh
        .vertices
        .iter()
        .map(|p| {
            let a = centre + (p[1].atan2(p[0]) - centre + PI).rem_euclid(TAU) - PI;
            [a * radius, p[2], p[0].hypot(p[1])]
        })
        .collect();
    let half = PI * radius;
    let triangles = mesh
        .triangles
        .iter()
        .filter(|t| {
            let s: Vec<f64> = t
                .iter()
                .filter_map(|&i| vertices.get(i as usize).map(|v| v[0]))
                .collect();
            s.len() == 3 && {
                let lo = s.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                hi - lo < half
            }
        })
        .copied()
        .collect();
    TriMesh {
        vertices,
        triangles,
    }
}
