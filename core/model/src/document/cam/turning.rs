// SPDX-License-Identifier: MIT
//! Turning setups in the document (mitcad#1869; docs/cam.md "Turning"): a
//! setup whose machine is a lathe turns its bodies about the spindle axis
//! with the crate's turning (`mitcad_cam::turning`).
//!
//! - **The axis** is the work Z: the setup's axis reference (a cylindrical
//!   face, an axis), else the design's Z through the middle of the
//!   bodies. Z points away from the chuck; X is the radial direction of
//!   the section (the work X).
//! - **The work zero** is on the axis at the part's face (the bodies'
//!   highest work Z), or at the Z of the setup's picked point.
//! - **The profile** is the bodies' section by the plane through the axis
//!   and the work X, its half on the work X side: the outside from the
//!   face to the back, the bore from its mouth inwards.
//! - **The stock** is a bar round the profile grown by the setup's
//!   margins (side: the radius; top: the face; bottom: the back), or a
//!   stock body's profile, with the setup's bore.

use std::f64::consts::{PI, TAU};

use serde_json::{Value, json};

use super::super::{Document, ModelError, invalid};
use super::{NO_MACHINE, hex, state};
use crate::cam::engine::turning::{
    self as turn, FaceOp, FinishOp, GrooveOp, PartOp, RoughOp, ThreadOp, TurnMove, TurnPart,
    TurnPath, TurnSide, TurnSpindle, TurningProgram, TurningProgramOp, TurningTool,
};
use crate::cam::engine::{self, Move, Toolpath};
use crate::cam::outline::{self, Seg, WorkFrame};
use crate::cam::{
    CamInput, CamMachine, CamOperation, CamSetup, CamState, CamValue, OperationKind, Stock,
    ValueKind, WcsOrigin,
};
use crate::datum::DatumPlane;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::transform::Transform;
use mitcad_vec3::{cross, norm, scale};

/// A lathe setup's frame, part and stock at the marker.
#[derive(Debug, Clone)]
pub(super) struct TurnFrame {
    /// Origin on the axis at the work zero, Z along the axis away from the
    /// chuck, X the section's radial direction.
    pub frame: WorkFrame,
    pub part: TurnPart,
    /// Where the stock ends towards the chuck (work Z).
    pub stock_back: f64,
    /// The tool change position: `[x (radius), z]`.
    pub change: [f64; 2],
}

/// One turning operation as computed.
pub(super) struct TurnComputed {
    pub id: u32,
    pub name: String,
    pub fingerprint: u128,
    /// None for a suppressed operation.
    pub result: Option<Result<TurningProgramOp, String>>,
}

/// What [`Document::compute_turning`] gives.
pub(super) struct TurnRun<'a> {
    pub frame: Result<TurnFrame, String>,
    pub machine: &'a CamMachine,
    pub operations: Vec<TurnComputed>,
    /// The sub-spindle's hand-over (mitcad#2319), when the setup has one.
    pub hand_over: Option<Result<turn::HandOver, String>>,
}

/// An operation on the main spindle after the sub-spindle's (mitcad#2392):
/// refused at Generate, as the program cannot be written so.
pub(super) const MAIN_AFTER_SUB: &str =
    "It is on the main spindle after the sub-spindle's operations: move it before them.";

/// The operations of a lathe setup on the main spindle after its first on
/// the sub-spindle (mitcad#2392), suppressed ones left out: their ids.
pub(super) fn main_after_sub(setup: &CamSetup) -> Vec<u32> {
    let live = || setup.operations.iter().filter(|o| !o.suppressed);
    let Some(first) = live().position(|o| o.sub_spindle) else {
        return Vec::new();
    };
    live()
        .skip(first)
        .filter(|o| !o.sub_spindle)
        .map(|o| o.id)
        .collect()
}

/// The inverse of a placement's linear part (a placement is invertible).
fn inverse(linear: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let m = linear;
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let d = if det.abs() < 1e-300 { 1.0 } else { det };
    let c =
        |r0: usize, c0: usize, r1: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    [
        [c(1, 1, 2, 2) / d, -c(0, 1, 2, 2) / d, c(0, 1, 1, 2) / d],
        [-c(1, 0, 2, 2) / d, c(0, 0, 2, 2) / d, -c(0, 0, 1, 2) / d],
        [c(1, 0, 2, 1) / d, -c(0, 0, 2, 1) / d, c(0, 0, 1, 1) / d],
    ]
}

fn apply(m: &[[f64; 3]; 3], v: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|r| m[r][0] * v[0] + m[r][1] * v[1] + m[r][2] * v[2])
}

/// The plane through the axis, in a body's own coordinates.
fn local_plane(frame: &WorkFrame, placement: &Transform) -> DatumPlane {
    let inv = inverse(&placement.linear);
    let o = apply(
        &inv,
        [
            frame.origin[0] - placement.translation[0],
            frame.origin[1] - placement.translation[1],
            frame.origin[2] - placement.translation[2],
        ],
    );
    let x = apply(&inv, frame.x);
    let z = apply(&inv, frame.z);
    let n = cross(x, z);
    let length = norm(n).max(1e-300);
    DatumPlane::from_normal(o, scale(n, 1.0 / length), Some(x))
}

/// The frame the section's segments are read in: X radial, Y along the
/// axis, so that arcs in the section are arcs about its Z.
fn profile_frame(frame: &WorkFrame) -> WorkFrame {
    WorkFrame {
        origin: frame.origin,
        x: frame.x,
        y: frame.z,
        z: cross(frame.x, frame.z),
    }
}

/// An arc segment's centre and radius in the plane (`[x, y]`).
fn arc_of(seg: &Seg) -> ([f64; 2], f64) {
    let (a, b) = ([seg.a[0], seg.a[1]], [seg.b[0], seg.b[1]]);
    let c = [b[0] - a[0], b[1] - a[1]];
    let d = c[0].hypot(c[1]).max(1e-300);
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0];
    let left = [-c[1] / d, c[0] / d];
    let h = d * (1.0 - seg.bulge * seg.bulge) / (4.0 * seg.bulge);
    let center = [mid[0] + left[0] * h, mid[1] + left[1] * h];
    (center, (a[0] - center[0]).hypot(a[1] - center[1]))
}

/// The segments' parts on the work X side of the axis (X at least zero).
fn half(segs: Vec<Seg>) -> Vec<Seg> {
    const EPS: f64 = 1e-9;
    let mut out = Vec::new();
    for seg in segs {
        let (ax, bx) = (seg.a[0], seg.b[0]);
        if ax >= -EPS
            && bx >= -EPS
            && (seg.bulge == 0.0 || {
                let (c, r) = arc_of(&seg);
                // An arc that bulges across the axis is split below.
                c[0] - r >= -EPS || !crosses_axis(&seg)
            })
        {
            out.push(seg);
            continue;
        }
        if seg.bulge == 0.0 {
            if ax <= EPS && bx <= EPS {
                continue;
            }
            let t = ax / (ax - bx);
            let p = [
                seg.a[0] + t * (seg.b[0] - seg.a[0]),
                seg.a[1] + t * (seg.b[1] - seg.a[1]),
                seg.a[2] + t * (seg.b[2] - seg.a[2]),
            ];
            let p = [0.0, p[1], p[2]];
            out.push(if ax > 0.0 {
                Seg {
                    a: seg.a,
                    b: p,
                    bulge: 0.0,
                }
            } else {
                Seg {
                    a: p,
                    b: seg.b,
                    bulge: 0.0,
                }
            });
            continue;
        }
        // An arc: split where it crosses the axis, keep the parts on the
        // positive side.
        let (c, r) = arc_of(&seg);
        let sweep = 4.0 * seg.bulge.atan();
        let a0 = (seg.a[1] - c[1]).atan2(seg.a[0] - c[0]);
        let along = |p: [f64; 2]| {
            let mut d = (p[1] - c[1]).atan2(p[0] - c[0]) - a0;
            if sweep > 0.0 {
                d = d.rem_euclid(TAU);
            } else {
                d = -((-d).rem_euclid(TAU));
            }
            d
        };
        let mut cuts: Vec<f64> = Vec::new();
        if r > c[0].abs() {
            let h = (r * r - c[0] * c[0]).sqrt();
            for y in [c[1] + h, c[1] - h] {
                let d = along([0.0, y]);
                if d.abs() > 1e-12 && d.abs() < sweep.abs() - 1e-12 {
                    cuts.push(d);
                }
            }
        }
        cuts.sort_by(|p, q| p.abs().total_cmp(&q.abs()));
        let mut bounds = vec![0.0];
        bounds.extend(cuts);
        bounds.push(sweep);
        let z = seg.a[2];
        let at = |d: f64| {
            let t = a0 + d;
            [c[0] + r * t.cos(), c[1] + r * t.sin(), z]
        };
        for w in bounds.windows(2) {
            let mid = at((w[0] + w[1]) / 2.0);
            if mid[0] <= EPS {
                continue;
            }
            let mut a = if w[0] == 0.0 { seg.a } else { at(w[0]) };
            let mut b = if w[1] == sweep { seg.b } else { at(w[1]) };
            if w[0] != 0.0 {
                a[0] = 0.0;
            }
            if w[1] != sweep {
                b[0] = 0.0;
            }
            out.push(Seg {
                a,
                b,
                bulge: ((w[1] - w[0]) / 4.0).tan(),
            });
        }
    }
    out
}

/// Whether an arc's sweep passes below X zero between its ends.
fn crosses_axis(seg: &Seg) -> bool {
    let (c, r) = arc_of(seg);
    if c[0] - r >= 0.0 {
        return false;
    }
    let sweep = 4.0 * seg.bulge.atan();
    let a0 = (seg.a[1] - c[1]).atan2(seg.a[0] - c[0]);
    // The angle of the arc's lowest X is PI.
    let mut d = PI - a0;
    if sweep > 0.0 {
        d = d.rem_euclid(TAU);
        d < sweep
    } else {
        d = -((-d).rem_euclid(TAU));
        d > sweep
    }
}

/// The outside and the bore of a half section: from the face (the
/// highest Z) the outside runs along the face outwards and on to the
/// back's outer corner, the bore the other way from the face's innermost
/// point.
fn split_profile(contour: &engine::Contour) -> Result<(Vec<turn::PV>, Vec<turn::PV>), String> {
    let v = &contour.vertices;
    let n = v.len();
    if n < 2 {
        return Err("The bodies' section about the spindle axis is empty.".to_owned());
    }
    let tol = 1e-6;
    let z_max = v.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
    let z_min = v.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let pick = |z: f64, smallest: bool| {
        (0..n)
            .filter(|&i| (v[i].y - z).abs() < tol)
            .min_by(|&i, &j| {
                let o = v[i].x.total_cmp(&v[j].x);
                if smallest { o } else { o.reverse() }
            })
    };
    let start = pick(z_max, true).ok_or("The bodies have no face to turn.")?;
    let back_out = pick(z_min, false).ok_or("The bodies have no back to turn.")?;
    let back_in = pick(z_min, true).ok_or("The bodies have no back to turn.")?;
    // The step from vertex i in a direction, as (next, bulge).
    let step = |i: usize, forward: bool| -> Option<(usize, f64)> {
        if forward {
            if i + 1 < n {
                Some((i + 1, v[i].bulge))
            } else if contour.closed {
                Some((0, v[i].bulge))
            } else {
                None
            }
        } else if i > 0 {
            Some((i - 1, -v[i - 1].bulge))
        } else if contour.closed {
            Some((n - 1, -v[n - 1].bulge))
        } else {
            None
        }
    };
    let walk = |forward: bool, stop: usize| -> Vec<turn::PV> {
        let mut out = vec![turn::PV::new(v[start].x, v[start].y)];
        let mut i = start;
        for _ in 0..n {
            let Some((next, bulge)) = step(i, forward) else {
                break;
            };
            if let Some(last) = out.last_mut() {
                last.bulge = bulge;
            }
            out.push(turn::PV::new(v[next].x, v[next].y));
            i = next;
            if i == stop || i == start {
                break;
            }
        }
        out
    };
    // The outside goes along the face outwards first.
    let outwards = |forward: bool| {
        step(start, forward).is_some_and(|(next, bulge)| {
            bulge == 0.0 && (v[next].y - z_max).abs() < tol && v[next].x > v[start].x
        })
    };
    let forward = if outwards(true) {
        true
    } else if outwards(false) {
        false
    } else {
        // No face along the axis: the outside is where X grows.
        step(start, true).is_some_and(|(next, _)| v[next].x >= v[start].x)
    };
    let outside = walk(forward, back_out);
    let inside = if v[start].x > tol {
        walk(!forward, back_in)
    } else {
        Vec::new()
    };
    Ok((outside, inside))
}

impl<K: Kernel> Document<K> {
    /// Whether a setup turns on a lathe.
    pub(super) fn cam_on_lathe(&self, setup: &CamSetup) -> bool {
        setup
            .machine
            .as_deref()
            .and_then(|m| self.state.cam.machine(m))
            .is_some_and(|m| m.machine.lathe.is_some())
    }

    /// The bodies' half section about the axis of `frame` in (radius, Z),
    /// chained.
    fn turn_section(
        &self,
        bodies: &[(BodyUid, Transform)],
        frame: &WorkFrame,
    ) -> Result<Vec<outline::Chain>, String> {
        let pf = profile_frame(frame);
        let mut segs = Vec::new();
        for (body, placement) in bodies {
            let Some(shape) = self.body_shape(*body) else {
                return Err(
                    "A body of the setup is not in the design at the timeline marker.".to_owned(),
                );
            };
            let plane = local_plane(frame, placement);
            let curves: Vec<crate::Curve3> = self
                .kernel()
                .section_curves(shape, None, &plane)
                .map_err(|_| "The bodies cannot be cut through the spindle axis.".to_owned())?
                .into_iter()
                .map(|c| c.curve)
                .collect();
            segs.extend(Self::cam_curve_segments(&curves, placement, &pf));
        }
        Ok(outline::chains(half(segs)))
    }

    /// A lathe setup's frame, part and stock at the marker.
    pub(super) fn turn_frame(&self, setup: &CamSetup) -> Result<TurnFrame, String> {
        let params = self.parameters();
        let length = |v: &CamValue| v.evaluate(params, ValueKind::Length);
        let bodies = self.cam_setup_bodies(setup)?;
        if bodies.is_empty() {
            return Err("The setup has no bodies to turn.".to_owned());
        }
        // The axis: its direction and a point on it.
        let (z, axis_point) = match &setup.wcs.z {
            None => ([0.0, 0.0, 1.0], None),
            Some(reference) => {
                let placement =
                    self.cam_placement(self.cam_ref_owner(reference), &setup.wcs.z_occurrence)?;
                match self.resolve_axis(reference) {
                    Ok(axis) => (
                        placement.apply_vector(axis.direction),
                        Some(placement.apply_point(axis.origin)),
                    ),
                    Err(_) => {
                        let (z, _) = self
                            .cam_direction(reference, &setup.wcs.z_occurrence, true)
                            .ok_or_else(|| {
                                "The spindle axis cannot be found: pick a cylindrical face or an axis."
                                    .to_owned()
                            })?;
                        (z, None)
                    }
                }
            }
        };
        let length_z = norm(z);
        if length_z < 1e-12 {
            return Err(
                "The spindle axis cannot be found: pick a cylindrical face or an axis.".to_owned(),
            );
        }
        let z = scale(z, 1.0 / length_z);
        let hint = match &setup.wcs.x {
            Some(reference) => Some(
                self.cam_direction(reference, &setup.wcs.x_occurrence, false)
                    .ok_or_else(|| {
                        "The work X cannot be found: pick a straight edge or an axis.".to_owned()
                    })?
                    .0,
            ),
            None => Some([1.0, 0.0, 0.0]),
        };
        let plane = DatumPlane::from_normal([0.0; 3], z, hint);
        let axes = WorkFrame {
            origin: [0.0; 3],
            x: plane.x_axis,
            y: plane.y_axis,
            z,
        };
        let (lo, hi) = self
            .cam_box(&bodies, &axes)
            .ok_or_else(|| "The setup has no bodies to turn.".to_owned())?;
        let on_axis = match axis_point {
            Some(p) => axes.to_work(p),
            None => [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, hi[2]],
        };
        let zero_z = match &setup.wcs.origin {
            WcsOrigin::Point { at, occurrence } => axes.to_work(self.cam_point(at, occurrence)?)[2],
            WcsOrigin::Stock { .. } => hi[2],
        };
        let frame = WorkFrame {
            origin: axes.to_model([on_axis[0], on_axis[1], zero_z]),
            ..axes
        };
        let chains = self.turn_section(&bodies, &frame)?;
        let widest =
            |c: &outline::Chain| c.contour.vertices.iter().map(|v| v.x).fold(0.0, f64::max);
        let chain = chains
            .iter()
            .max_by(|a, b| widest(a).total_cmp(&widest(b)))
            .ok_or_else(|| {
                "The bodies do not reach the spindle axis's plane: pick the axis they turn about."
                    .to_owned()
            })?;
        let (outside, inside) = split_profile(&chain.contour)?;
        let face = outside.first().map_or(0.0, |v| v.z);
        let back = outside.last().map_or(0.0, |v| v.z);
        let radius = outside.iter().map(|v| v.x).fold(0.0, f64::max);
        let (stock_radius, stock_face, stock_back) = match &setup.stock {
            Stock::AroundBodies { side, top, bottom } => (
                radius + length(side)?,
                face + length(top)?,
                back - length(bottom)?,
            ),
            Stock::Body { body, occurrence } => {
                let placed = self.cam_body_placed(&crate::cam::CamBody {
                    body: *body,
                    occurrence: occurrence.clone(),
                })?;
                let chains = self.turn_section(&[placed], &frame)?;
                let pts = chains.iter().flat_map(|c| c.contour.vertices.iter());
                let (mut r, mut zf, mut zb) = (0.0_f64, f64::NEG_INFINITY, f64::INFINITY);
                for p in pts {
                    r = r.max(p.x);
                    zf = zf.max(p.y);
                    zb = zb.min(p.y);
                }
                if !zf.is_finite() {
                    return Err(
                        "The stock body does not reach the spindle axis's plane.".to_owned()
                    );
                }
                (r, zf.max(face), zb)
            }
        };
        let lathe = setup.lathe.clone().unwrap_or_default();
        let bore = match &lathe.stock_bore {
            Some(v) => length(v)? / 2.0,
            None => 0.0,
        };
        let part = TurnPart {
            outside,
            inside,
            stock_radius,
            stock_inner_radius: bore,
            stock_face,
        };
        part.validate()?;
        let change = [
            match &lathe.change_diameter {
                Some(v) => length(v)? / 2.0,
                None => stock_radius + 50.0,
            },
            match &lathe.change_z {
                Some(v) => length(v)?,
                None => stock_face + 50.0,
            },
        ];
        Ok(TurnFrame {
            frame,
            part,
            stock_back,
            change,
        })
    }

    /// The Z range and the radius (the largest, or the smallest when
    /// `smallest`) of a face's trace in the section.
    fn turn_face_extent(
        &self,
        tf: &TurnFrame,
        input: &CamInput,
        smallest: bool,
    ) -> Result<([f64; 2], f64), String> {
        let CamInput::Face { body, face, .. } = input else {
            return Err("Pick the face the operation cuts.".to_owned());
        };
        let shape = self.body_shape(*body).ok_or_else(|| {
            "A body the operation reads is not in the design at the timeline marker.".to_owned()
        })?;
        let placement = self.cam_input_placement(input)?;
        let plane = local_plane(&tf.frame, &placement);
        let curves: Vec<crate::Curve3> = self
            .kernel()
            .section_curves(shape, Some(face), &plane)
            .map_err(|_| "The picked face cannot be cut through the spindle axis.".to_owned())?
            .into_iter()
            .map(|c| c.curve)
            .collect();
        let segs = half(Self::cam_curve_segments(
            &curves,
            &placement,
            &profile_frame(&tf.frame),
        ));
        if segs.is_empty() {
            return Err("The picked face is not on the turned profile.".to_owned());
        }
        let mut z = [f64::INFINITY, f64::NEG_INFINITY];
        let mut x = if smallest { f64::INFINITY } else { 0.0 };
        for s in &segs {
            for p in [s.a, s.b] {
                z[0] = z[0].min(p[1]);
                z[1] = z[1].max(p[1]);
                x = if smallest { x.min(p[0]) } else { x.max(p[0]) };
            }
        }
        Ok((z, x))
    }

    /// One turning operation's toolpath and its program entry.
    fn turn_compute_operation(
        &self,
        tf: &TurnFrame,
        machine: &CamMachine,
        op: &CamOperation,
    ) -> Result<TurningProgramOp, String> {
        use ValueKind::{Length, Number};
        let tool = machine.tools.get(op.tool).ok_or_else(|| {
            "The operation's tool is not in the machine's tool library.".to_owned()
        })?;
        let engine_tool: TurningTool = tool.turning_engine()?;
        let t = op.turning.clone().unwrap_or_default();
        let part = &tf.part;
        let clearance = self
            .cam_value(&op.heights.clearance, Length)?
            .unwrap_or(2.0);
        let retract = self.cam_value(&op.heights.retract, Length)?.unwrap_or(0.5);
        let feed_default = match op.kind {
            OperationKind::TurnFace => 0.2,
            OperationKind::TurnRough { .. } => 0.25,
            OperationKind::TurnFinish { .. } => 0.1,
            _ => 0.05,
        };
        let feed = self
            .cam_value(&t.feed_per_rev, Length)?
            .unwrap_or(feed_default);
        let depth = self
            .cam_value(&t.depth_of_cut, Length)?
            .unwrap_or_else(|| engine_tool.max_depth.min(2.0));
        let peck = self.cam_value(&t.peck, Length)?.unwrap_or(0.0);
        let leave = self.cam_value(&op.stock_to_leave, Length)?;
        let leave_z = self.cam_value(&t.stock_to_leave_z, Length)?;
        let max_rpm = self
            .cam_value(&t.max_rpm, Number)?
            .unwrap_or(machine.machine.spindle.max_rpm);
        let speed_default = match op.kind {
            OperationKind::TurnRough {
                side: TurnSide::Inside,
            }
            | OperationKind::TurnFinish {
                side: TurnSide::Inside,
            } => 150.0,
            OperationKind::TurnGroove { .. } | OperationKind::TurnThread { .. } => 100.0,
            OperationKind::TurnPartOff { .. } => 80.0,
            _ => 200.0,
        };
        let surface_speed = self
            .cam_value(&t.surface_speed, Number)?
            .unwrap_or(speed_default);
        let rpm_given = self.cam_value(&op.cutting.spindle_rpm, Number)?;
        let rpm_at = |radius: f64| {
            rpm_given.unwrap_or_else(|| {
                (surface_speed * 1000.0 / (PI * (2.0 * radius).max(1.0)))
                    .min(max_rpm)
                    .round()
            })
        };
        let css = TurnSpindle::Css {
            surface_speed,
            max_rpm,
        };
        let constant = |radius: f64| TurnSpindle::Rpm {
            rpm: rpm_at(radius),
        };
        let spindle_for = |radius: f64| {
            if t.constant_rpm {
                constant(radius)
            } else {
                css
            }
        };
        let (path, spindle): (Result<TurnPath, engine::CamError>, TurnSpindle) = match &op.kind {
            OperationKind::TurnFace => (
                turn::face(
                    part,
                    &FaceOp {
                        step: depth,
                        clearance,
                        feed,
                    },
                    &engine_tool,
                ),
                spindle_for(part.stock_radius),
            ),
            OperationKind::TurnRough { side } => {
                let allowance = leave.unwrap_or(0.2);
                (
                    turn::rough(
                        part,
                        &RoughOp {
                            side: *side,
                            depth,
                            retract,
                            allowance_x: allowance,
                            allowance_z: leave_z.unwrap_or(allowance / 2.0),
                            clearance,
                            feed,
                        },
                        &engine_tool,
                    ),
                    spindle_for(part.stock_radius),
                )
            }
            OperationKind::TurnFinish { side } => (
                turn::finish(
                    part,
                    &FinishOp {
                        side: *side,
                        clearance,
                        feed,
                    },
                    &engine_tool,
                ),
                spindle_for(part.stock_radius),
            ),
            OperationKind::TurnGroove { side, z_from, z_to } => {
                let (from, to) = match (
                    self.cam_value(z_from, Length)?,
                    self.cam_value(z_to, Length)?,
                ) {
                    (Some(a), Some(b)) => (a, b),
                    _ => {
                        let input = op.inputs.first().ok_or_else(|| {
                            "Pick the groove's bottom face, or give the groove's Z from and to."
                                .to_owned()
                        })?;
                        let (z, _) = self.turn_face_extent(tf, input, false)?;
                        (z[1], z[0])
                    }
                };
                (
                    turn::groove(
                        part,
                        &GrooveOp {
                            side: *side,
                            z_from: from,
                            z_to: to,
                            peck,
                            clearance,
                            feed,
                        },
                        &engine_tool,
                    ),
                    spindle_for(part.stock_radius),
                )
            }
            OperationKind::TurnPartOff { z } => (
                turn::part_off(
                    part,
                    &PartOp {
                        z: self.cam_value(z, Length)?,
                        to_x: 0.0,
                        peck,
                        clearance,
                        feed,
                    },
                    &engine_tool,
                ),
                spindle_for(part.stock_radius),
            ),
            OperationKind::TurnThread {
                side,
                pitch,
                diameter,
                z_start,
                z_end,
                depth: thread_depth,
            } => {
                let pitch = pitch.evaluate(self.parameters(), Length)?;
                let inside = *side == TurnSide::Inside;
                let given = (
                    self.cam_value(diameter, Length)?,
                    self.cam_value(z_start, Length)?,
                    self.cam_value(z_end, Length)?,
                );
                let (crest, start, end) = match given {
                    (Some(d), Some(a), Some(b)) => (d / 2.0, a, b),
                    _ => {
                        let input = op.inputs.first().ok_or_else(|| {
                            "Pick the threaded face, or give the thread's diameter, start and end."
                                .to_owned()
                        })?;
                        let (z, x) = self.turn_face_extent(tf, input, inside)?;
                        (
                            given.0.map_or(x, |d| d / 2.0),
                            given.1.unwrap_or(z[1]),
                            given.2.unwrap_or(z[0]),
                        )
                    }
                };
                let height = match self.cam_value(thread_depth, Length)? {
                    Some(h) => h,
                    None if inside => turn::ops::internal_thread_height(pitch),
                    None => turn::ops::external_thread_height(pitch),
                };
                let first = self
                    .cam_value(&t.depth_of_cut, Length)?
                    .unwrap_or((height / 3.0).max(0.05));
                (
                    turn::thread(
                        &ThreadOp {
                            side: *side,
                            crest,
                            z_start: start,
                            z_end: end,
                            lead: pitch,
                            height,
                            first_depth: first,
                            min_depth: (height / 10.0).min(0.05),
                            finish_allowance: (height / 20.0).min(0.05),
                            spring_passes: 1,
                            run_in: 3.0,
                            clearance,
                        },
                        &engine_tool,
                    ),
                    // Threading runs at a constant speed.
                    constant(crest),
                )
            }
            _ => {
                return Err("A lathe setup takes turning operations only.".to_owned());
            }
        };
        Ok(TurningProgramOp {
            name: op.name.clone(),
            tool: engine_tool,
            spindle,
            direction: op.spindle,
            coolant: op.coolant,
            path: path.map_err(|e| e.message)?,
            live: None,
            sub: false,
        })
    }

    /// Computes a lathe setup's operations (all, or those of `only`).
    pub(super) fn compute_turning(
        &self,
        index: usize,
        only: Option<&[u32]>,
    ) -> Result<TurnRun<'_>, ModelError> {
        let setup = &self.state.cam.setups[index];
        let machine = setup
            .machine
            .as_deref()
            .and_then(|m| self.state.cam.machine(m))
            .ok_or_else(|| invalid(NO_MACHINE))?;
        let frame = self.turn_frame(setup);
        let sf = self.cam_setup_frame(setup);
        // The part turned round on the sub-spindle (mitcad#2319).
        let sub_frame = match &frame {
            Ok(tf) if setup.operations.iter().any(|o| o.sub_spindle) => {
                Some(self.turn_sub_frame(setup, tf))
            }
            _ => None,
        };
        let hand_over = match (
            &frame,
            setup.lathe.as_ref().and_then(|l| l.hand_over.as_ref()),
        ) {
            (Ok(tf), Some(h)) => Some(self.turn_hand_over(tf, h)),
            _ => None,
        };
        // The main spindle's operations after the sub-spindle's are refused
        // here, at Generate, not only when the program is written
        // (mitcad#2392).
        let misplaced = main_after_sub(setup);
        let operations = setup
            .operations
            .iter()
            .filter(|op| only.is_none_or(|ids| ids.contains(&op.id)))
            .map(|op| {
                let fingerprint = self.cam_fingerprint(setup, &sf, Some(machine), op);
                let result = (!op.suppressed).then(|| match (&frame, &sub_frame) {
                    _ if misplaced.contains(&op.id) => Err(MAIN_AFTER_SUB.to_owned()),
                    (Err(e), _) => Err(e.clone()),
                    (Ok(_), Some(sub)) if op.sub_spindle => {
                        let sub = sub.as_ref().map_err(Clone::clone)?;
                        // Turning, or a live tool on the sub-spindle's C
                        // axis (mitcad#2375), in its work coordinates.
                        let mut turned = if op.kind.is_turning() {
                            self.turn_compute_operation(sub, machine, op)?
                        } else {
                            self.live_compute_operation(setup, sub, machine, op)?
                        };
                        turned.sub = true;
                        Ok(turned)
                    }
                    (Ok(tf), _) if op.kind.is_turning() => {
                        self.turn_compute_operation(tf, machine, op)
                    }
                    // A live tool on the C axis (mitcad#2067).
                    (Ok(tf), _) => self.live_compute_operation(setup, tf, machine, op),
                });
                TurnComputed {
                    id: op.id,
                    name: op.name.clone(),
                    fingerprint,
                    result,
                }
            })
            .collect();
        Ok(TurnRun {
            frame,
            machine,
            operations,
            hand_over,
        })
    }

    /// The sub-spindle's work coordinates of a lathe setup (mitcad#2319):
    /// the setup's frame turned round about its X at the part's back (the
    /// zero there, Z away from the sub-spindle's chuck), the part's
    /// section in it, the stock from where the main spindle's parting off
    /// cut it (the last operation before the first on the sub-spindle,
    /// else the stock's back) to the part's face.
    pub(super) fn turn_sub_frame(
        &self,
        setup: &CamSetup,
        tf: &TurnFrame,
    ) -> Result<TurnFrame, String> {
        let back = tf.part.back_z();
        let frame = WorkFrame {
            origin: tf.frame.to_model([0.0, 0.0, back]),
            x: tf.frame.x,
            y: scale(tf.frame.y, -1.0),
            z: scale(tf.frame.z, -1.0),
        };
        let bodies = self.cam_setup_bodies(setup)?;
        let chains = self.turn_section(&bodies, &frame)?;
        let widest =
            |c: &outline::Chain| c.contour.vertices.iter().map(|v| v.x).fold(0.0, f64::max);
        let chain = chains
            .iter()
            .max_by(|a, b| widest(a).total_cmp(&widest(b)))
            .ok_or_else(|| "The part's back cannot be cut through the spindle axis.".to_owned())?;
        let (outside, inside) = split_profile(&chain.contour)?;
        let live = |o: &&CamOperation| !o.suppressed;
        let first = setup
            .operations
            .iter()
            .position(|o| o.sub_spindle && !o.suppressed);
        let parting = first.and_then(|i| setup.operations[..i].iter().rev().find(live));
        let cut = match parting.map(|o| &o.kind) {
            Some(OperationKind::TurnPartOff { z }) => match z {
                Some(v) => v.evaluate(self.parameters(), ValueKind::Length)?,
                None => back,
            },
            _ => tf.stock_back,
        };
        let face = outside.first().map_or(0.0, |v| v.z);
        let part = TurnPart {
            outside,
            inside,
            stock_radius: tf.part.stock_radius,
            stock_inner_radius: tf.part.stock_inner_radius,
            stock_face: (back - cut).max(face),
        };
        part.validate()?;
        let clear = tf.change[1] - tf.part.stock_face;
        let change = [tf.change[0], part.stock_face + clear];
        Ok(TurnFrame {
            frame,
            part,
            stock_back: back - tf.part.stock_face,
            change,
        })
    }

    /// A lathe setup's hand-over to the sub-spindle with its defaults
    /// (mitcad#2319, [`crate::cam::HandOverSetup`]).
    fn turn_hand_over(
        &self,
        tf: &TurnFrame,
        h: &crate::cam::HandOverSetup,
    ) -> Result<turn::HandOver, String> {
        let params = self.parameters();
        let value = |v: &Option<CamValue>, kind: ValueKind, default: f64| match v {
            Some(v) => v.evaluate(params, kind),
            None => Ok(default),
        };
        let (face, back) = (tf.part.face_z(), tf.part.back_z());
        let hand_over = turn::HandOver {
            grip_z: value(
                &h.grip_z,
                ValueKind::Length,
                face - (0.5 * (face - back)).min(10.0),
            )?,
            withdraw: value(&h.withdraw, ValueKind::Length, 50.0)?,
            rpm: value(&h.rpm, ValueKind::Number, 500.0)?,
            feed: value(&h.feed, ValueKind::Length, 1000.0)?,
            work_offset: h.work_offset.unwrap_or(2),
            back_z: back,
        };
        hand_over.validate().map_err(|e| e.message)?;
        Ok(hand_over)
    }

    /// The program of a computed lathe setup; refused when an operation
    /// failed.
    pub(super) fn turn_program(
        setup: &CamSetup,
        run: &TurnRun<'_>,
    ) -> Result<TurningProgram, String> {
        let tf = run.frame.as_ref().map_err(Clone::clone)?;
        let mut program = TurningProgram::new(&setup.name, tf.change);
        program.work_offset = setup.work_offset;
        if let Some(number) = setup.lathe.as_ref().and_then(|l| l.program_number) {
            program.number = number;
        }
        // The lathe's codes of the C axis and its live tools (mitcad#2067).
        if let Some(lathe) = &run.machine.machine.lathe {
            program.mill_turn = lathe.mill_turn.clone();
        }
        // The sub-spindle's hand-over (mitcad#2319).
        if let Some(hand_over) = &run.hand_over {
            program.hand_over = Some(hand_over.clone()?);
        }
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

    /// The lathe check's findings of a program, when the setup has the
    /// work zero's machine position.
    fn turn_findings(
        &self,
        setup: &CamSetup,
        run: &TurnRun<'_>,
        program: &TurningProgram,
    ) -> Value {
        let (Some(origin), Ok(tf)) = (setup.placement, &run.frame) else {
            return json!([]);
        };
        let chuck_z = setup
            .lathe
            .as_ref()
            .and_then(|l| l.chuck_gap.as_ref())
            .and_then(|v| v.evaluate(self.parameters(), ValueKind::Length).ok())
            .map(|gap| tf.stock_back - gap);
        let placement = turn::check::TurnPlacement {
            work_z: origin[2],
            chuck_z,
        };
        json!(turn::check_lathe(&run.machine.machine, program, &placement))
    }

    /// A turning toolpath for the view: its moves in work coordinates in
    /// the section's plane (X radial, Y zero, Z along the axis), arcs as
    /// short lines; feeds per revolution.
    fn turn_view_path(path: &TurnPath) -> Toolpath {
        let mut moves = Vec::new();
        let mut at: Option<[f64; 2]> = None;
        for m in &path.moves {
            let pts = match at {
                Some(from) => turn::move_points(from, m, 0.005),
                None => vec![m.to()],
            };
            for p in pts {
                let to = [p[0], 0.0, p[1]];
                moves.push(match *m {
                    TurnMove::Rapid { .. } => Move::Rapid { to },
                    TurnMove::Line { feed, .. } | TurnMove::Arc { feed, .. } => {
                        Move::Line { to, feed }
                    }
                    TurnMove::Thread { lead, .. } => Move::Line { to, feed: lead },
                });
            }
            at = Some(m.to());
        }
        Toolpath {
            moves,
            cycles: Vec::new(),
        }
    }

    /// A lathe setup's frame as JSON (model coordinates): the milling
    /// setups' fields, the stock as a box about the axis, and the turned
    /// part's profile.
    pub(super) fn turn_frame_json(frame: &Result<TurnFrame, String>) -> Value {
        match frame {
            Ok(tf) => {
                let r = tf.part.stock_radius;
                json!({
                    "origin": tf.frame.origin,
                    "x": tf.frame.x,
                    "y": tf.frame.y,
                    "z": tf.frame.z,
                    "stock_min": [-r, -r, tf.stock_back],
                    "stock_max": [r, r, tf.part.stock_face],
                    "bodies_top": tf.part.face_z(),
                    "safe_z": tf.change[1],
                    "turning": {
                        "part": tf.part,
                        "stock_back": tf.stock_back,
                        "change": tf.change,
                    },
                })
            }
            Err(e) => json!({"error": e}),
        }
    }

    /// `cam_generate` of a lathe setup: records the fingerprints in `cam`
    /// and gives the setup's entry.
    pub(super) fn generate_turning(
        &self,
        index: usize,
        cam: &mut CamState,
    ) -> Result<Value, ModelError> {
        let run = self.compute_turning(index, None)?;
        let setup = &self.state.cam.setups[index];
        let rapid = run.machine.machine.rapid_feed;
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
                    if let Some(live) = &op.live {
                        // A live tool's moves, feeds per minute (mitcad#2067).
                        entry["cut_length"] = json!(live.path.cut_length());
                        entry["duration"] = json!(live.path.duration(rapid));
                        entry["moves"] = json!(live.path.moves.len());
                    } else {
                        entry["cut_length"] = json!(op.path.cut_length());
                        entry["duration"] =
                            json!(op.path.duration(rapid, |x| op.spindle.rpm_at(x)));
                        entry["moves"] = json!(op.path.moves.len());
                    }
                }
                Some(Err(e)) => {
                    entry["state"] = json!("failed");
                    entry["error"] = json!(e);
                }
            }
            operations.push(entry);
        }
        let findings = match Self::turn_program(setup, &run) {
            Ok(program) => self.turn_findings(setup, &run, &program),
            Err(_) => json!([]),
        };
        let mut value = json!({
            "id": setup.id,
            "name": setup.name,
            "operations": operations,
            "findings": findings,
            "turning": true,
        });
        if let Err(e) = &run.frame {
            value["error"] = json!(e);
        }
        Ok(value)
    }

    /// `cam_toolpaths` of a lathe setup.
    pub(super) fn turning_toolpaths_json(
        &self,
        index: usize,
        operations: Option<&[u32]>,
    ) -> Result<Value, ModelError> {
        let setup = &self.state.cam.setups[index];
        let run = self.compute_turning(index, operations)?;
        let rapid = run.machine.machine.rapid_feed;
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
                    // A live tool's moves in the work coordinates
                    // (mitcad#2067).
                    Some(Ok(op)) if op.live.is_some() => {
                        let live = op.live.as_ref().expect("a live operation");
                        let mut view = Self::live_view_path(live);
                        // On the sub-spindle (mitcad#2375): on the part
                        // where it was, turned round at its back.
                        if let (true, Ok(tf)) = (op.sub, &run.frame) {
                            let back = tf.part.back_z();
                            for m in &mut view.moves {
                                if let Move::Rapid { to } | Move::Line { to, .. } = m {
                                    *to = [to[0], -to[1], back - to[2]];
                                }
                            }
                            value["sub_spindle"] = json!(true);
                        }
                        value["bounds"] = json!(view.bounds());
                        value["toolpath"] = json!(view);
                        value["live"] = json!({"side": live.side, "path": live.path});
                        value["tool"] = json!({"number": op.tool.number, "name": op.tool.name,
                                               "diameter": live.cutter.diameter});
                        value["spindle"] = json!(op.spindle);
                        value["cut_length"] = json!(live.path.cut_length());
                        value["duration"] = json!(live.path.duration(rapid));
                    }
                    Some(Ok(op)) => {
                        let mut view = Self::turn_view_path(&op.path);
                        // The sub-spindle's moves on the part where it
                        // was (mitcad#2319): its Z turned round at the
                        // part's back.
                        if let (true, Ok(tf)) = (op.sub, &run.frame) {
                            let back = tf.part.back_z();
                            for m in &mut view.moves {
                                if let Move::Rapid { to } | Move::Line { to, .. } = m {
                                    *to = [to[0], -to[1], back - to[2]];
                                }
                            }
                            value["sub_spindle"] = json!(true);
                        }
                        value["bounds"] = json!(view.bounds());
                        value["toolpath"] = json!(view);
                        value["turn_path"] = json!(op.path);
                        value["tool"] = json!({"number": op.tool.number, "name": op.tool.name,
                                               "diameter": 2.0 * op.tool.nose_radius.max(0.2)});
                        value["spindle"] = json!(op.spindle);
                        value["cut_length"] = json!(op.path.cut_length());
                        value["duration"] =
                            json!(op.path.duration(rapid, |x| op.spindle.rpm_at(x)));
                    }
                    Some(Err(e)) => value["error"] = json!(e),
                }
                value
            })
            .collect();
        let findings = match Self::turn_program(setup, &run) {
            Ok(program) => self.turn_findings(setup, &run, &program),
            Err(_) => json!([]),
        };
        Ok(json!({
            "setup": setup.id,
            "work": Self::turn_frame_json(&run.frame),
            "operations": ops,
            "findings": findings,
            "turning": true,
        }))
    }

    /// `cam_post` of a lathe setup: the program in its turning dialect,
    /// checked by reading it back.
    pub(super) fn turning_post_json(
        &self,
        index: usize,
        dialect: Option<&str>,
        units: Option<engine::Units>,
    ) -> Result<Value, ModelError> {
        let setup = &self.state.cam.setups[index];
        let run = self.compute_turning(index, None)?;
        let id = dialect
            .map(str::to_owned)
            .or_else(|| setup.dialect.clone())
            .unwrap_or_else(|| run.machine.machine.controller.clone());
        let post = engine::turning_dialect(&id)
            .ok_or_else(|| invalid("MitCad does not write lathe G-code for that controller."))?;
        let program = Self::turn_program(setup, &run).map_err(invalid)?;
        let mut settings = setup.post.clone();
        if let Some(units) = units {
            settings.units = units;
        }
        let text = engine::turning_post(&program, post.as_ref(), &settings)
            .map_err(|e| invalid(e.message))?;
        // Read back with the lathe's own codes (the sub-spindle's stop,
        // mitcad#2375).
        let check = turn::check_program_with(&text, &program.mill_turn).map_err(|e| {
            invalid(format!(
                "The written program does not read back as meant: {}",
                e.message
            ))
        })?;
        Ok(json!({
            "text": text,
            "extension": post.extension(),
            "dialect": post.id(),
            "label": post.label(),
            "units": settings.units,
            "cycles": check.cycles,
        }))
    }

    /// `cam_simulate` of a lathe setup: the turned stock's section (radii
    /// per Z, `step` apart), the material left over the part and the
    /// rapids that cut.
    pub(super) fn turning_simulate_json(
        &self,
        index: usize,
        resolution: Option<f64>,
        tolerance: Option<f64>,
    ) -> Result<Value, ModelError> {
        let setup = &self.state.cam.setups[index];
        let run = self.compute_turning(index, None)?;
        let program = Self::turn_program(setup, &run).map_err(invalid)?;
        let tf = run.frame.as_ref().map_err(|e| invalid(e.clone()))?;
        let step = resolution
            .filter(|r| r.is_finite() && *r > 0.0)
            .unwrap_or(0.05);
        let tolerance = tolerance
            .filter(|t| t.is_finite() && *t >= 0.0)
            .unwrap_or(crate::cam::simulate::REMAINING_TOLERANCE);
        // Room for the parting blade beyond the stock's back.
        let z_min = tf.stock_back.min(tf.part.back_z()) - 10.0;
        let sim = turn::simulate_turning(&tf.part, &program, z_min, step, tolerance);
        // The live tools' cuts on the face and the diameter (mitcad#2067),
        // each program operation by its id.
        let ids: Vec<u32> = run
            .operations
            .iter()
            .filter(|c| matches!(c.result, Some(Ok(_))))
            .map(|c| c.id)
            .collect();
        let live = self
            .live_simulate_json(setup, tf, &program, &ids, (z_min, step), tolerance)
            .map_err(invalid)?;
        // The whole program cut out of the bar in 3D (mitcad#2319).
        let solid = self
            .solid_simulate_json(setup, tf, &program, &ids, (z_min, step))
            .map_err(invalid)?;
        // The moves that run into the sub-spindle's body or a zone moving
        // with it, where it is as each operation cuts (mitcad#2392); those
        // of the main spindle's operations only with the work zero's
        // machine place.
        let sub_hits = turn::sub_spindle_hits(
            &run.machine.machine,
            &program,
            setup.placement.map(|p| p[2]),
        );
        // The sub-spindle as Simulate draws it (mitcad#2405): its outline
        // and where its jaws' face is as each operation (its id) cuts, on
        // the part where the view draws it; none without a body.
        let sub_spindle = run
            .machine
            .machine
            .lathe
            .as_ref()
            .filter(|l| l.axes.sub_spindle)
            .map(|l| (l, l.sub_spindle.outline()))
            .filter(|(_, outline)| !outline.is_empty())
            .map_or(Value::Null, |(lathe, outline)| {
                let work_z = setup.placement.map(|p| p[2]);
                let faces: Vec<Value> = ids
                    .iter()
                    .enumerate()
                    .map(|(i, id)| {
                        json!({"id": id,
                               "face": turn::sub_face_shown(lathe, &program, i, work_z)})
                    })
                    .collect();
                json!({"outline": outline, "operations": faces})
            });
        Ok(json!({
            "setup": setup.id,
            "turning": true,
            "work": Self::turn_frame_json(&run.frame),
            "stock": sim.stock,
            "left": sim.left,
            "left_at": sim.left_at,
            "gouges": sim.gouges,
            "rapid_cuts": sim.rapid_cuts,
            "live": live,
            "solid": solid,
            "sub_spindle_hits": sub_hits,
            "sub_spindle": sub_spindle,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cam::engine::{Contour, Vertex};

    fn seg(a: [f64; 2], b: [f64; 2], bulge: f64) -> Seg {
        Seg {
            a: [a[0], a[1], 0.0],
            b: [b[0], b[1], 0.0],
            bulge,
        }
    }

    #[test]
    fn the_half_section_keeps_the_work_x_side() {
        // A face line across the axis and a ball end across it.
        let segs = vec![
            seg([-10.0, 0.0], [10.0, 0.0], 0.0),
            seg([5.0, -20.0], [-5.0, -20.0], 1.0),
        ];
        let kept = half(segs);
        assert_eq!(kept.len(), 2, "{kept:?}");
        assert_eq!(kept[0].a, [0.0, 0.0, 0.0]);
        assert_eq!(kept[0].b, [10.0, 0.0, 0.0]);
        // The half circle from (5, -20) round through (0, -15) is a
        // quarter on this side.
        assert!((kept[1].bulge - (PI / 8.0).tan()).abs() < 1e-9, "{kept:?}");
        assert!(kept[1].b[0].abs() < 1e-9 && (kept[1].b[1] + 15.0).abs() < 1e-9);
    }

    #[test]
    fn a_bored_shaft_splits_into_outside_and_bore() {
        // Its half section: the face from the bore (6) out to 10, the
        // outside to Z -30, the back in to the bore, the bore back to the
        // face.
        let contour = Contour::closed(vec![
            Vertex::line(6.0, 0.0),
            Vertex::line(10.0, 0.0),
            Vertex::line(10.0, -30.0),
            Vertex::line(6.0, -30.0),
        ]);
        let (outside, inside) = split_profile(&contour).unwrap();
        let pts = |c: &[turn::PV]| c.iter().map(|v| [v.x, v.z]).collect::<Vec<_>>();
        assert_eq!(pts(&outside), vec![[6.0, 0.0], [10.0, 0.0], [10.0, -30.0]]);
        assert_eq!(pts(&inside), vec![[6.0, 0.0], [6.0, -30.0]]);
        // A solid one: no bore.
        let contour = Contour::open(vec![
            Vertex::line(0.0, -30.0),
            Vertex::line(10.0, -30.0),
            Vertex::line(10.0, 0.0),
            Vertex::line(0.0, 0.0),
        ]);
        let (outside, inside) = split_profile(&contour).unwrap();
        assert_eq!(pts(&outside), vec![[0.0, 0.0], [10.0, 0.0], [10.0, -30.0]]);
        assert!(inside.is_empty());
    }
}
