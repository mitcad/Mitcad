// SPDX-License-Identifier: MIT
//! What recompute does with motion links (mitcad#466; the feature is
//! `features/motion_link.rs`): at its point of the timeline a link works
//! out how its follower follows its driver (a ratio from its numbers,
//! expressions or picked round geometry, or a cam's table) and solves its
//! two joints together with the joints and links that share moving
//! occurrences with them (`joints::solve_new`): the joint solver
//! (`mitcad_solver::RigidCoupling`) holds `b = relation(a)`, so Drive
//! Joint, drags, Animate Joint and motion studies move the linked joints
//! together, and a contact of a contact set stops the whole motion.
//!
//! A cam's table: the cam joint `a` is revolute; its profile (a planar
//! face's outer boundary, or the sketch profile that the named curves
//! bound) is on the side that turns. In the cam's fixed frame C (the
//! other side's frame) the profile turns by a's value about z. The
//! follower joint `b` slides (a translating follower: its line is its
//! slide axis through its tip) or turns about an axis along C's z (an
//! oscillating one: its arm from the pivot to its tip). Its guide is the
//! side of `b` on the part the cam turns against (else `b`'s own side
//! `b`). Over a turn of the cam the follower stands where it first
//! touches the profile (`third_party/solidcraft/cam.rs`); the table is its
//! value there with its tip where it is now along its line (or arm), so
//! that it touches the cam; the link's offset is added to it.

use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};
use std::sync::Arc;

use mitcad_solver::Relation;

use crate::assembly::{Assembly, inverse};
use crate::datum::{CurveGeometry, SurfaceGeometry, Vec3};
use crate::document::DocState;
use crate::features::geom_ref::{GeomRef, Resolver};
use crate::features::joint::JointOrigin;
use crate::features::{
    Coupling, FeatureEntry, Follower, LinkSize, LinkedMotion, MotionLinkDef, PlacementChange,
};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{
    ActiveJoint, JointState, Motion, StepResolver, apply_override, origin_frame, path_component,
    path_placement, solve_new,
};
use crate::kernel::{Curve3, Kernel};
use crate::parameters::{ParamId, Parameters};
use crate::profile::SegmentGeometry;
use crate::recompute::{BodyState, Recomputed};
use crate::topo::{CurveId, TopoName};
use crate::transform::Transform;

#[path = "../../../third_party/solidcraft/cam.rs"]
mod cam;

pub use cam::CAM_SAMPLES;

/// A motion link in effect: the joints' motions it couples and how.
#[derive(Debug, Clone, PartialEq)]
#[doc(hidden)]
pub struct ActiveLink {
    pub uid: FeatureUid,
    pub component: ComponentUid,
    pub a: (FeatureUid, Motion),
    pub b: (FeatureUid, Motion),
    pub relation: Relation,
}

impl ActiveLink {
    /// How far b's value is from what the relation gives for a's (a turn
    /// of the follower within a turn; a free driving turn at the turn that
    /// fits best).
    pub(crate) fn error(&self, va: f64, vb: f64) -> f64 {
        let off = |x: f64| {
            let e = vb - self.relation.follow(x);
            if self.b.1.is_rotation() {
                ((e + PI).rem_euclid(TAU) - PI).abs()
            } else {
                e.abs()
            }
        };
        if !self.a.1.is_rotation() {
            return off(va);
        }
        (-8..=8)
            .map(|k| off(va + f64::from(k) * TAU))
            .fold(f64::INFINITY, f64::min)
    }
}

/// What a motion link did in the last recompute.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkResult {
    pub component: ComponentUid,
    pub a: LinkedMotion,
    pub b: LinkedMotion,
    /// `ratio`, `gear`, `rack_pinion`, `screw` or `cam`.
    pub coupling: &'static str,
    /// b's value per a's (mm or radians each) of a linear coupling.
    pub ratio: Option<f64>,
    /// Added to the follower's value (a cam's with its tip's place).
    pub offset: f64,
    pub state: JointState,
    /// The joints' values at the marker, when both hold there.
    pub values: Option<(f64, f64)>,
    /// Whether b follows a at the marker.
    pub holds: bool,
    pub placements: Vec<PlacementChange>,
}

/// Geometry a link names, resolved in the components its occurrence paths
/// end in and placed into the link's component.
pub(crate) struct LinkGeo<'a, R: Resolver, F: FnMut(ComponentUid) -> R> {
    pub assembly: &'a Assembly,
    pub component: ComponentUid,
    pub placements: &'a BTreeMap<OccurrenceUid, Transform>,
    pub resolver: F,
}

impl<R: Resolver, F: FnMut(ComponentUid) -> R> LinkGeo<'_, R, F> {
    /// The resolver of the component an origin's path ends in, and the
    /// path's placement.
    fn at(&mut self, origin: &JointOrigin) -> Result<(R, Transform), String> {
        let at = path_component(self.assembly, self.component, &origin.occurrence.0)?;
        Ok((
            (self.resolver)(at),
            path_placement(self.placements, &origin.occurrence.0),
        ))
    }

    /// An origin's frame in the link's component.
    fn frame(&mut self, origin: &JointOrigin) -> Result<Transform, String> {
        let (mut r, placed) = self.at(origin)?;
        let mut local = origin_frame(&mut r, &origin.geometry)?;
        if let Some(o) = &origin.frame_override {
            local = apply_override(&local, o)?;
        }
        Ok(placed.after(&local))
    }

    /// The radius of round geometry: a cylindrical, conical or spherical
    /// face, a circular edge, a sketch circle or arc.
    fn radius(&mut self, origin: &JointOrigin) -> Result<f64, String> {
        let (mut r, _) = self.at(origin)?;
        let geometry = &origin.geometry;
        let radius = match geometry {
            GeomRef::Face { body, face } => {
                let shape = r.shape(*body)?;
                let face = crate::features::renamed::face_for(&mut r, &shape, face)?;
                match r.kernel().face_geometry(&shape, &face).map_err(|e| {
                    crate::notice::about_place(
                        crate::notice::NoticePlace::face(face.to_string()),
                        &e.to_string(),
                    )
                })? {
                    SurfaceGeometry::Cylinder { radius, .. }
                    | SurfaceGeometry::Cone { radius, .. }
                    | SurfaceGeometry::Sphere { radius, .. } => radius,
                    other => {
                        return Err(crate::notice::placed(
                            "the face is not round",
                            crate::notice::NoticePlace::face(face.to_string()),
                            format!("face {face} ({}) is not round", other.kind()),
                        ));
                    }
                }
            }
            GeomRef::Edge { body, edge } => {
                let shape = r.shape(*body)?;
                let edge = crate::features::renamed::edge_for(&mut r, &shape, edge)?;
                match r.kernel().edge_geometry(&shape, &edge).map_err(|e| {
                    crate::notice::about_place(
                        crate::notice::NoticePlace::edge(edge.to_string()),
                        &e.to_string(),
                    )
                })? {
                    CurveGeometry::Circle { radius, .. } => radius,
                    other => {
                        return Err(crate::notice::placed(
                            "the edge is not round",
                            crate::notice::NoticePlace::edge(edge.to_string()),
                            format!("edge {edge} ({}) is not round", other.kind()),
                        ));
                    }
                }
            }
            GeomRef::SketchCurves { sketch, curves } if curves.len() == 1 => {
                let output = r.sketch(*sketch)?;
                match output.solved.curves.get(&curves[0]) {
                    Some(
                        crate::sketch::geometry::Curve2::Circle { radius, .. }
                        | crate::sketch::geometry::Curve2::Arc { radius, .. },
                    ) => *radius,
                    _ => return Err(format!("{geometry} is not a circle or an arc")),
                }
            }
            other => {
                return Err(format!(
                    "{other} has no radius (a round face, a circular edge or a sketch circle)"
                ));
            }
        };
        if radius.is_finite() && radius > 0.0 {
            Ok(radius)
        } else {
            Err(format!("{geometry} has no radius"))
        }
    }

    /// A cam profile's polylines in the link's component: a sketch
    /// profile's outer loop (one closed polyline), or a face's boundary
    /// edges (to be joined into loops).
    fn profile(&mut self, origin: &JointOrigin, tol: f64) -> Result<Vec<Vec<Vec3>>, String> {
        let (mut r, placed) = self.at(origin)?;
        let local: Vec<Vec<Vec3>> = match &origin.geometry {
            GeomRef::SketchCurves { sketch, curves } => {
                let output = r.sketch(*sketch)?;
                let named = |key: &CurveId| matches!(key, CurveId::Curve(c) if curves.contains(c));
                let shared = |i: usize| {
                    output.regions[i].loops.first().map_or(0, |l| {
                        l.segments.iter().filter(|s| named(&s.key.curve)).count()
                    })
                };
                let area = |i: usize| {
                    output
                        .region_info
                        .iter()
                        .find(|info| info.profile.key == output.regions[i].key)
                        .map_or(0.0, |info| info.area)
                };
                let best = (0..output.regions.len())
                    .filter(|i| curves.is_empty() || 2 * shared(*i) >= curves.len())
                    .max_by(|a, b| {
                        shared(*a)
                            .cmp(&shared(*b))
                            .then(area(*a).total_cmp(&area(*b)))
                    })
                    .ok_or_else(|| {
                        format!("{} has no closed profile of these curves", r.name(*sketch))
                    })?;
                let region = &output.regions[best];
                let outer = region.loops.first().ok_or("the profile has no boundary")?;
                let mut points: Vec<[f64; 2]> = Vec::new();
                for segment in &outer.segments {
                    let Some(mut s) = sample_segment(&segment.geometry, tol) else {
                        // A periodic spline: the region's own outline.
                        points = output
                            .region_info
                            .iter()
                            .find(|info| info.profile.key == region.key)
                            .map(|info| info.outline.clone())
                            .unwrap_or_default();
                        break;
                    };
                    if let Some(last) = points.last().copied() {
                        let d = |p: [f64; 2]| (p[0] - last[0]).hypot(p[1] - last[1]);
                        if let (Some(first), Some(end)) = (s.first(), s.last())
                            && d(*end) < d(*first)
                        {
                            s.reverse();
                        }
                        s.remove(0);
                    }
                    points.extend(s);
                }
                let frame = output.frame;
                vec![points.into_iter().map(|p| frame.point(p)).collect()]
            }
            GeomRef::Face { body, face } => {
                let shape = r.shape(*body)?;
                let face = crate::features::renamed::face_for(&mut r, &shape, face)?;
                let curves = r
                    .kernel()
                    .curves_of(&shape, &TopoName::Face(face.clone()))
                    .map_err(|e| {
                        crate::notice::about_place(
                            crate::notice::NoticePlace::face(face.to_string()),
                            &e.to_string(),
                        )
                    })?;
                curves
                    .iter()
                    .map(|c| sample_curve(c, tol))
                    .filter(|p| p.len() > 1)
                    .collect()
            }
            other => return Err(format!("{other} is no cam profile")),
        };
        Ok(local
            .into_iter()
            .map(|line| line.into_iter().map(|p| placed.apply_point(p)).collect())
            .collect())
    }
}

/// Points along an arc of `radius` through `sweep` radians, within `tol`.
fn steps(radius: f64, sweep: f64, tol: f64) -> usize {
    let step = if radius > tol {
        2.0 * (1.0 - tol / radius).clamp(-1.0, 1.0).acos()
    } else {
        sweep
    };
    ((sweep.abs() / step.max(1e-4)).ceil() as usize).clamp(2, 4000)
}

/// A sketch profile segment's points in its direction, within `tol`;
/// None for a periodic spline.
fn sample_segment(g: &SegmentGeometry, tol: f64) -> Option<Vec<[f64; 2]>> {
    let ellipse = |c: [f64; 2], a: f64, b: f64, rot: f64, t0: f64, t1: f64| {
        let n = steps(a.max(b), t1 - t0, tol);
        (0..=n)
            .map(|i| {
                let t = t0 + (t1 - t0) * i as f64 / n as f64;
                let (x, y) = (a * t.cos(), b * t.sin());
                [
                    c[0] + x * rot.cos() - y * rot.sin(),
                    c[1] + x * rot.sin() + y * rot.cos(),
                ]
            })
            .collect::<Vec<_>>()
    };
    let ccw = |s: f64, e: f64| if e <= s { e + TAU } else { e };
    Some(match g {
        SegmentGeometry::Line { start, end } => vec![*start, *end],
        SegmentGeometry::Arc {
            center,
            radius,
            start_angle,
            end_angle,
        } => ellipse(
            *center,
            *radius,
            *radius,
            0.0,
            *start_angle,
            ccw(*start_angle, *end_angle),
        ),
        SegmentGeometry::Circle {
            center,
            radius,
            start,
        } => ellipse(*center, *radius, *radius, 0.0, *start, start + TAU),
        SegmentGeometry::Ellipse {
            center,
            major_radius,
            minor_radius,
            rotation,
        } => ellipse(*center, *major_radius, *minor_radius, *rotation, 0.0, TAU),
        SegmentGeometry::EllipseArc {
            center,
            major_radius,
            minor_radius,
            rotation,
            start_angle,
            end_angle,
        } => ellipse(
            *center,
            *major_radius,
            *minor_radius,
            *rotation,
            *start_angle,
            ccw(*start_angle, *end_angle),
        ),
        SegmentGeometry::BSpline {
            degree,
            poles,
            weights,
            knots,
            multiplicities,
            periodic,
        } => {
            if *periodic {
                return None;
            }
            let full: Vec<f64> = knots
                .iter()
                .zip(multiplicities)
                .flat_map(|(k, m)| std::iter::repeat_n(*k, *m as usize))
                .collect();
            let poles3: Vec<Vec3> = poles.iter().map(|p| [p[0], p[1], 0.0]).collect();
            sample_bspline(*degree as usize, &poles3, weights, &full)?
                .into_iter()
                .map(|p| [p[0], p[1]])
                .collect()
        }
    })
}

fn sample_bspline(
    degree: usize,
    poles: &[Vec3],
    weights: &[f64],
    knots: &[f64],
) -> Option<Vec<Vec3>> {
    if poles.len() <= degree || knots.len() != poles.len() + degree + 1 {
        return None;
    }
    let (t0, t1) = (knots[degree], knots[poles.len()]);
    let n = (poles.len() * 24).clamp(16, 4000);
    Some(
        (0..=n)
            .map(|i| {
                let t = t0 + (t1 - t0) * i as f64 / n as f64;
                crate::features::path::bspline_point(degree, poles, weights, knots, t)
            })
            .collect(),
    )
}

/// An edge curve's points, within `tol`.
fn sample_curve(c: &Curve3, tol: f64) -> Vec<Vec3> {
    match c {
        Curve3::Point(_) => Vec::new(),
        Curve3::Line { start, end } => vec![*start, *end],
        Curve3::Conic {
            center,
            normal,
            x_axis,
            major,
            minor,
            start,
            end,
            closed,
        } => {
            let (t0, t1) = if *closed {
                (*start, start + TAU)
            } else {
                (*start, *end)
            };
            let n = steps(major.max(*minor), t1 - t0, tol);
            (0..=n)
                .map(|i| {
                    let t = t0 + (t1 - t0) * i as f64 / n as f64;
                    crate::features::path::conic_point(
                        *center,
                        *normal,
                        *x_axis,
                        (*major, *minor),
                        t,
                    )
                })
                .collect()
        }
        Curve3::BSpline {
            degree,
            poles,
            weights,
            knots,
        } => sample_bspline(*degree as usize, poles, weights, knots).unwrap_or_default(),
        Curve3::Helix {
            center,
            axis,
            x_axis,
            radius,
            pitch,
            start,
            end,
        } => {
            let n = steps(*radius, end - start, tol);
            (0..=n)
                .map(|i| {
                    let t = start + (end - start) * i as f64 / n as f64;
                    crate::features::path::helix_point(
                        *center,
                        *axis,
                        *x_axis,
                        (*radius, *pitch),
                        t,
                    )
                })
                .collect()
        }
    }
}

/// Polylines joined end to end into closed loops (within `join`).
fn loops(mut lines: Vec<Vec<[f64; 2]>>, join: f64) -> Vec<Vec<[f64; 2]>> {
    let d = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let mut out = Vec::new();
    while let Some(mut path) = lines.pop() {
        while let (Some(first), Some(end)) = (path.first().copied(), path.last().copied()) {
            if path.len() > 2 && d(first, end) <= join {
                path.pop();
                out.push(path);
                break;
            }
            let Some((i, flip)) = lines.iter().enumerate().find_map(|(i, l)| {
                if d(l[0], end) <= join {
                    Some((i, false))
                } else if d(*l.last()?, end) <= join {
                    Some((i, true))
                } else {
                    None
                }
            }) else {
                break;
            };
            let mut next = lines.swap_remove(i);
            if flip {
                next.reverse();
            }
            path.extend(next.into_iter().skip(1));
        }
    }
    out
}

/// A parameter's value, or zero without one.
fn value_of(params: &Parameters, id: Option<&ParamId>) -> Result<f64, String> {
    match id {
        None => Ok(0.0),
        Some(id) => params
            .value(*id)
            .ok_or_else(|| format!("parameter {} does not exist", params.name(*id))),
    }
}

/// How a link's follower follows its driver where the joints are, and a
/// linear coupling's ratio.
pub(crate) fn relation<R: Resolver, F: FnMut(ComponentUid) -> R>(
    geo: &mut LinkGeo<'_, R, F>,
    params: &Parameters,
    link: &MotionLinkDef,
    (ja, jb): (&ActiveJoint, &ActiveJoint),
) -> Result<(Relation, Option<f64>), String> {
    let size = |geo: &mut LinkGeo<'_, R, F>, s: &LinkSize<ParamId>| -> Result<f64, String> {
        let v = match s {
            LinkSize::Value(id) => value_of(params, Some(id))?,
            LinkSize::Round(o) => geo.radius(o)?,
        };
        if v.is_finite() && v > 0.0 {
            Ok(v)
        } else {
            Err(format!(
                "a gear, rack, screw or roller needs a positive size, not {v}"
            ))
        }
    };
    let (turn_a, turn_b) = (link.a.motion.is_rotation(), link.b.motion.is_rotation());
    // Travel per radian of a turn and a slide, either way round.
    let travel = |k: f64| if turn_a { k } else { 1.0 / k };
    let sign = if link.reverse { -1.0 } else { 1.0 };
    let offset = value_of(params, link.offset.as_ref())?;
    let ratio = match &link.coupling {
        Coupling::Ratio { ratio } => value_of(params, Some(ratio))?,
        Coupling::Gear { ra, rb, internal } => {
            let r = size(geo, ra)? / size(geo, rb)?;
            if *internal { r } else { -r }
        }
        Coupling::RackPinion { radius } => {
            if turn_a == turn_b {
                return Err("a rack and pinion couples a turn and a slide".to_owned());
            }
            travel(size(geo, radius)?)
        }
        Coupling::Screw { lead } => {
            if turn_a == turn_b {
                return Err("a screw couples a turn and a slide".to_owned());
            }
            travel(size(geo, lead)? / TAU)
        }
        Coupling::Cam {
            profile,
            follower,
            tip,
        } => {
            let radius = match follower {
                Follower::Roller { radius } => size(geo, radius)?,
                _ => 0.0,
            };
            let flat = matches!(follower, Follower::Flat);
            // A roller picked by its round face is its tip too.
            let roller_at = match follower {
                Follower::Roller {
                    radius: LinkSize::Round(o),
                } => Some(&**o),
                _ => None,
            };
            let tip = tip.as_deref().or(roller_at);
            let (values, auto) = cam_table(geo, link, (ja, jb), profile, tip, radius, flat)?;
            return Ok((
                Relation::Table {
                    values,
                    offset: auto + offset,
                },
                None,
            ));
        }
    } * sign;
    if !ratio.is_finite() || ratio == 0.0 {
        return Err(format!(
            "the ratio must be a number other than zero, not {ratio}"
        ));
    }
    Ok((Relation::Linear { ratio, offset }, Some(ratio)))
}

fn top(path: &[OccurrenceUid]) -> Option<OccurrenceUid> {
    path.first().copied()
}

fn xy(p: Vec3) -> [f64; 2] {
    [p[0], p[1]]
}

/// A cam's table (see the module documentation) and the follower's
/// offset that puts its tip where it is now.
fn cam_table<R: Resolver, F: FnMut(ComponentUid) -> R>(
    geo: &mut LinkGeo<'_, R, F>,
    link: &MotionLinkDef,
    (ja, jb): (&ActiveJoint, &ActiveJoint),
    profile: &JointOrigin,
    tip: Option<&JointOrigin>,
    radius: f64,
    flat: bool,
) -> Result<(Vec<f64>, f64), String> {
    let bad = |m: &str| format!("cam: {m}");
    let placements = geo.placements;
    let (fa, fb) = ja.frames(placements);
    // The side the profile turns with, the cam's fixed frame C and the
    // map from the link's coordinates to the profile at a's value 0.
    let on = top(&profile.occurrence.0);
    let (turn, c, to_zero, fixed_top) = if on == top(&ja.a) {
        (1.0, fb, ja.alignment.after(&inverse(&fa)), top(&ja.b))
    } else if on == top(&ja.b) {
        (
            -1.0,
            fa.after(&inverse(&ja.alignment)),
            inverse(&fb),
            top(&ja.a),
        )
    } else {
        return Err(bad("the profile must be on a part the cam joint turns"));
    };
    let c_inv = inverse(&c);
    let lines = geo.profile(profile, 1e-4)?;
    let mut flat_lines: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut zs: Vec<f64> = Vec::new();
    for line in &lines {
        let mapped: Vec<Vec3> = line.iter().map(|p| to_zero.apply_point(*p)).collect();
        zs.extend(mapped.iter().map(|p| p[2]));
        flat_lines.push(mapped.into_iter().map(xy).collect());
    }
    let all: Vec<[f64; 2]> = flat_lines.iter().flatten().copied().collect();
    let size = {
        let (mut lo, mut hi) = ([f64::INFINITY; 2], [f64::NEG_INFINITY; 2]);
        for p in &all {
            for i in 0..2 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        if all.is_empty() {
            0.0
        } else {
            (hi[0] - lo[0]).hypot(hi[1] - lo[1])
        }
    };
    if !(size.is_finite() && size > 0.0) {
        return Err(bad("the profile has no points"));
    }
    let mean = zs.iter().sum::<f64>() / zs.len() as f64;
    if zs.iter().any(|z| (z - mean).abs() > 1e-6 * size + 1e-9) {
        return Err(bad(
            "the profile must lie in a plane square to the cam's axis",
        ));
    }
    let poly = loops(flat_lines, 1e-6 * size + 1e-6)
        .into_iter()
        .max_by(|a, b| cam::area(a).abs().total_cmp(&cam::area(b).abs()))
        .ok_or_else(|| bad("the profile is not a closed loop"))?;
    if poly.len() < 3 || poly.len() > cam::MAX_PROFILE_POINTS {
        return Err(bad(&format!(
            "the profile must have 3 to {} points",
            cam::MAX_PROFILE_POINTS
        )));
    }
    if cam::area(&poly).abs() <= 1e-12 * size * size {
        return Err(bad("the profile encloses no area"));
    }

    // The follower's guide: its joint's side on the part the cam turns
    // against, else side b.
    let (ga, gb) = jb.frames(placements);
    let guide_is_a = top(&jb.a) == fixed_top && top(&jb.b) != fixed_top;
    let (guide, moving, gain) = if guide_is_a {
        (ga.after(&inverse(&jb.alignment)), gb, -1.0)
    } else {
        (gb, ga, 1.0)
    };
    let guide_top = if guide_is_a { top(&jb.a) } else { top(&jb.b) };
    if guide_top.is_some() && guide_top == on {
        return Err(bad(
            "the follower's guide turns with the cam's profile (join it to the part the cam turns against)",
        ));
    }
    let to_c = c_inv.after(&guide);
    let g = to_c.apply_point([0.0; 3]);
    let tip_now = match tip {
        Some(o) => c_inv.after(&geo.frame(o)?).apply_point([0.0; 3]),
        None => c_inv.after(&moving).apply_point([0.0; 3]),
    };
    let vb_now = jb
        .values(placements)
        .and_then(|v| v.get(&link.b.motion).copied())
        .ok_or_else(|| bad("the follower's joint does not hold"))?;
    let va_now = ja
        .values(placements)
        .and_then(|v| v.get(&link.a.motion).copied())
        .ok_or_else(|| bad("the cam's joint does not hold"))?;
    let side_flip = if link.reverse { -1.0 } else { 1.0 };
    let sliding = !link.b.motion.is_rotation();
    let setup = if sliding {
        let axis = match link.b.motion {
            Motion::Tx => [1.0, 0.0, 0.0],
            Motion::Ty => [0.0, 1.0, 0.0],
            _ => [0.0, 0.0, 1.0],
        };
        let d = to_c.apply_vector(axis);
        if d[2].abs() > 1e-6 {
            return Err(bad("the follower must slide square to the cam's axis"));
        }
        let len = d[0].hypot(d[1]);
        let dir = [d[0] / len, d[1] / len];
        let perp = [-dir[1], dir[0]];
        let across = (tip_now[0] - g[0]) * perp[0] + (tip_now[1] - g[1]) * perp[1];
        let at = [g[0] + perp[0] * across, g[1] + perp[1] * across];
        // From the side of the cam's axis the tip is on now.
        let side = if tip_now[0] * dir[0] + tip_now[1] * dir[1] >= 0.0 {
            side_flip
        } else {
            -side_flip
        };
        cam::CamSetup {
            poly,
            turn,
            at,
            dir,
            arm: None,
            side,
            gain,
            radius,
            flat,
            size,
        }
    } else {
        if flat {
            return Err(bad("a flat-faced follower slides (on a slider)"));
        }
        let z = to_c.apply_vector([0.0, 0.0, 1.0]);
        if z[0].hypot(z[1]) > 1e-6 {
            return Err(bad(
                "the follower must swing about an axis parallel to the cam's",
            ));
        }
        if tip.is_none() {
            return Err(bad(
                "a swinging follower needs its tip (the knife's point or the roller's center)",
            ));
        }
        let pivot = xy(g);
        if pivot[0].hypot(pivot[1]) <= 1e-9 * size {
            return Err(bad("the follower's pivot is on the cam's axis"));
        }
        let arm = [tip_now[0] - pivot[0], tip_now[1] - pivot[1]];
        if arm[0].hypot(arm[1]) <= 1e-9 * size {
            return Err(bad("the follower's tip is on its pivot"));
        }
        let off = (arm[1].atan2(arm[0]) - pivot[1].atan2(pivot[0]) + PI).rem_euclid(TAU) - PI;
        let side = if off < 0.0 { -side_flip } else { side_flip };
        cam::CamSetup {
            poly,
            turn,
            at: pivot,
            dir: [1.0, 0.0],
            arm: Some(arm),
            side,
            gain: gain * z[2].signum(),
            radius,
            flat,
            size,
        }
    };
    let table = cam::table(&setup).map_err(|e| bad(&e))?;
    // Where the tip is now, in the table's measure.
    let now = if sliding {
        (tip_now[0] - setup.at[0]) * setup.dir[0] + (tip_now[1] - setup.at[1]) * setup.dir[1]
    } else {
        let arm = setup.arm.expect("a swinging follower's arm");
        let raw = arm[1].atan2(arm[0]);
        // The turn nearest to the table's angle there.
        let lifted = Relation::Table {
            values: table.clone(),
            offset: 0.0,
        }
        .follow(va_now)
            / setup.gain;
        raw + ((lifted - raw) / TAU).round() * TAU
    };
    Ok((table, vb_now - setup.gain * now))
}

/// The joint a link names, in effect in its component.
fn active_joint<'a>(
    active: &'a [ActiveJoint],
    component: ComponentUid,
    m: &LinkedMotion,
    name: &dyn Fn(FeatureUid) -> String,
) -> Result<&'a ActiveJoint, String> {
    active
        .iter()
        .find(|j| j.uid == m.joint && j.row.is_none() && j.component == component)
        .ok_or_else(|| {
            format!(
                "{} is not in effect (suppressed, failed or after this link)",
                name(m.joint)
            )
        })
}

/// Applies a motion link at its point of the recompute: works out its
/// relation and solves its joints together (see the module
/// documentation).
pub(crate) fn apply<K: Kernel>(
    kernel: &K,
    state: &DocState,
    entry: &FeatureEntry,
    link: &MotionLinkDef,
    components: &BTreeMap<ComponentUid, Arc<BodyState<K::Shape>>>,
    done: &mut Recomputed<K::Shape>,
) -> Result<Vec<String>, String> {
    let component = entry.component;
    let name = |uid: FeatureUid| crate::joints::relation_name(state, uid, None);
    let ja = active_joint(&done.joints.active, component, &link.a, &name)
        .map_err(|e| format!("a: {e}"))?
        .clone();
    let jb = active_joint(&done.joints.active, component, &link.b, &name)
        .map_err(|e| format!("b: {e}"))?
        .clone();
    let (relation, ratio) = {
        let datums = &done.datums;
        let sketches = &done.sketches;
        let history = &done.results;
        let mut geo = LinkGeo {
            assembly: &state.assembly,
            component,
            placements: &done.placements,
            resolver: |at: ComponentUid| StepResolver {
                kernel,
                assembly: &state.assembly,
                component: at,
                bodies: components.get(&at).map(|b| &**b),
                datums,
                sketches,
                features: &state.features,
                history,
                renamed: Vec::new(),
            },
        };
        relation(&mut geo, &state.parameters, link, (&ja, &jb))?
    };
    let offset = match &relation {
        Relation::Linear { offset, .. } | Relation::Table { offset, .. } => *offset,
    };
    let active = ActiveLink {
        uid: entry.uid,
        component,
        a: (link.a.joint, link.a.motion),
        b: (link.b.joint, link.b.motion),
        relation,
    };
    let (state_now, changes) = solve_new(state, entry, &[], std::slice::from_ref(&active), done)?;
    done.joints.links.push(active);
    done.joints.link_results.insert(
        entry.uid,
        LinkResult {
            component,
            a: link.a,
            b: link.b,
            coupling: link.coupling.type_name(),
            ratio,
            offset,
            state: state_now,
            values: None,
            holds: true,
            placements: changes,
        },
    );
    Ok(Vec::new())
}

/// After recompute: whether each link's follower follows its driver at the
/// marker, from the joints' values there.
pub(crate) fn finish(joints: &mut crate::joints::JointsState) {
    let value = |uid: FeatureUid, m: Motion| {
        joints
            .joints
            .get(&uid)
            .and_then(|r| r.values.as_ref())
            .and_then(|v| v.get(&m).copied())
    };
    let mut results = std::mem::take(&mut joints.link_results);
    for l in &joints.links {
        let values = value(l.a.0, l.a.1).zip(value(l.b.0, l.b.1));
        let holds = values.is_some_and(|(va, vb)| {
            let scale = if l.b.1.is_rotation() {
                1.0
            } else {
                1.0 + vb.abs()
            };
            l.error(va, vb) <= 1e-6 * scale
        });
        if let Some(r) = results.get_mut(&l.uid) {
            r.values = values;
            r.holds = holds;
        }
    }
    joints.link_results = results;
}

/// The links in a document's definitions (not only those in effect) that
/// couple a joint's motion, with the other side: (link, other joint,
/// other motion).
pub(crate) fn coupled(
    state: &DocState,
    joint: FeatureUid,
    motion: Motion,
) -> Vec<(FeatureUid, FeatureUid, Motion)> {
    let mut out = Vec::new();
    for f in state.features.iter().filter(|f| !f.suppressed) {
        let crate::features::FeatureDef::MotionLink(l) = &f.def else {
            continue;
        };
        if l.a.joint == joint && l.a.motion == motion {
            out.push((f.uid, l.b.joint, l.b.motion));
        }
        if l.b.joint == joint && l.b.motion == motion {
            out.push((f.uid, l.a.joint, l.a.motion));
        }
    }
    out
}
