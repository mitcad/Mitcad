// SPDX-License-Identifier: MIT
//! What sets the diameter of a thread's face that does not fit the thread
//! (mitcad#1632): the value or dimension Auto Fix and the Thread panel's
//! *Fit the hole to the thread* change so that the face takes the
//! thread's crest diameter (an internal thread's minor diameter, an
//! external one's major diameter, as the fit check of mitcad#1262 compares
//! them).
//!
//! - A Hole's wall: the hole's diameter (a simple hole: not tapped,
//!   tapered, a clearance or a press-fit hole, whose sizes come from
//!   elsewhere).
//! - A Cylinder's round side: its diameter.
//! - The side an Extrude (or Cut) makes of a sketch circle or arc, or a
//!   Revolve of a sketch line along its axis: the one driving dimension of
//!   the sketch that moves the curve's radius, or its distance from the
//!   axis. Every active driving dimension of the sketch is tried with its
//!   value moved a little and the sketch solved again; the face's diameter
//!   must come from exactly one of them (a diameter, a radius, a distance
//!   from a point or a line on the axis, ...). The dimension's value that
//!   gives the crest diameter is then solved for.
//! - Anything else (an imported body, a face a fillet or a boolean made, a
//!   face whose diameter comes from several dimensions or from none) has
//!   no source: the thread's warning says the face has to be changed by
//!   hand, and why. A face of a projected curve (a reference curve that
//!   follows the model geometry it was projected from) says so
//!   (mitcad#1780): no dimension of its sketch moves it.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::features::{FeatureDef, FeatureEntry, ProfileRef, SketchDef};
use crate::ids::{BodyUid, EntityUid, FeatureUid};
use crate::kernel::Cylinder;
use crate::parameters::{ParamId, Parameters};
use crate::profile::SketchFrame;
use crate::sketch::geometry::Curve2;
use crate::sketch::solve::Solved;
use crate::topo::{CurveId, FaceName, RoleKey, SegmentKey};

/// An axis in a sketch's plane: a point and a unit direction.
type Axis2 = ([f64; 2], [f64; 2]);

/// What sets a face's diameter: the value of `parameter` (a hole's or a
/// cylinder's diameter, a sketch dimension) of `feature`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FitSource {
    /// The Hole, the Cylinder or the sketch whose value it is.
    pub feature: FeatureUid,
    /// The parameter's id ([`ParamId::raw`]).
    pub parameter: u32,
    /// The value (mm or rad) the parameter must have for the face to take
    /// the thread's crest diameter.
    pub value: f64,
}

impl FitSource {
    pub fn param(&self) -> ParamId {
        ParamId::from_raw(self.parameter)
    }
}

/// A face of a thread whose diameter lies too far from the thread's crest
/// diameter (the fit warning of mitcad#1262), with what sets it
/// (mitcad#1632).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadFitMiss {
    pub body: BodyUid,
    pub face: FaceName,
    /// The face's diameter (mm).
    pub diameter: f64,
    /// The diameter the thread needs (mm): its minor diameter on a hole,
    /// its major diameter on a shaft.
    pub needed: f64,
    /// A hole's wall (the material outside).
    pub internal: bool,
    /// The warning the thread gave for it.
    pub warning: String,
    /// What sets the face's diameter; None when nothing Auto Fix can
    /// change does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<FitSource>,
    /// Why there is no source, in words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// How far (mm) a measured diameter may lie from the face's before the
/// source is not believed (the face was moved or changed after it was
/// made), and how close a solve with the value found must come to the
/// crest. The solver may leave about 1e-5 mm on a sketch 100 mm across
/// (its looser second pass, `LOOSE_TOLERANCE` in mitcad-sketch): the
/// value itself is worked out from the face's measured diameter, not from
/// a solve.
const MATCH: f64 = 1e-3;
const REACH: f64 = 1e-3;
/// How much of a dimension's step must reach the diameter for the
/// dimension to move it: far above the solver's noise, far below any
/// slope a dimension that sets it has (a radius 2, a distance from the
/// axis 2, a diameter 1, a linear diameter 1).
const MOVES: f64 = 1e-2;
/// The slopes a dimension that sets a diameter by itself has: a diameter
/// or a linear diameter 1, a radius or a distance from the axis 2, each
/// either way; a measured slope this close to one of them is that one.
const SLOPES: [f64; 4] = [1.0, 2.0, -1.0, -2.0];
const SLOPE_MATCH: f64 = 1e-3;

/// What sets the diameter of `face` (its `cylinder` as the thread measured
/// it) so that it can be made `needed`: see the module's docs. `features`
/// are the timeline's, `params` the parameters, `frame` a sketch's frame
/// as last computed. The error says why there is none, in words.
pub fn fit_source(
    features: &[Arc<FeatureEntry>],
    params: &Parameters,
    frame: &dyn Fn(FeatureUid) -> Option<SketchFrame>,
    face: &FaceName,
    cylinder: &Cylinder,
    needed: f64,
) -> Result<FitSource, String> {
    let entry = features
        .iter()
        .find(|f| f.uid == face.feature)
        .ok_or_else(|| "the feature that made the face is not in the timeline".to_owned())?;
    let diameter = 2.0 * cylinder.radius;
    let direct = |id: ParamId| -> Result<FitSource, String> {
        let value = params
            .value(id)
            .ok_or_else(|| format!("{}'s diameter has no value", entry.name))?;
        if (value - diameter).abs() > MATCH {
            return Err(format!(
                "the face is no longer as {}'s diameter made it",
                entry.name
            ));
        }
        Ok(FitSource {
            feature: entry.uid,
            parameter: id.raw(),
            value: needed,
        })
    };
    match (&entry.def, face.role.as_str(), &face.key) {
        (FeatureDef::Hole(hole), role, _) if is_hole_wall(role) => {
            if hole.thread.is_some() {
                Err(format!("{}'s thread size sets its diameter", entry.name))
            } else if hole.taper.is_some() {
                Err(format!("{} is tapered", entry.name))
            } else if hole.clearance.is_some() {
                Err(format!("{}'s screw size sets its diameter", entry.name))
            } else if hole.press_fit.is_some() {
                Err(format!("{} is a press-fit hole", entry.name))
            } else {
                direct(hole.diameter)
            }
        }
        (FeatureDef::Cylinder(c), "side0", None) => direct(c.diameter),
        (FeatureDef::Extrude(e), "side", Some(RoleKey::Segment(segment))) => sketch_source(
            features,
            params,
            frame,
            &e.profiles,
            segment,
            cylinder,
            needed,
        ),
        (FeatureDef::Revolve(r), "side", Some(RoleKey::Segment(segment))) => sketch_source(
            features,
            params,
            frame,
            &r.profiles,
            segment,
            cylinder,
            needed,
        ),
        _ => Err(format!(
            "the face was made by {} ({}), not from a dimension",
            entry.name,
            entry.def.type_name()
        )),
    }
}

/// A hole's wall: `hole<n>.wall`.
fn is_hole_wall(role: &str) -> bool {
    role.strip_prefix("hole")
        .and_then(|rest| rest.strip_suffix(".wall"))
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The face of a sketch curve (`segment`) one of `profiles` names: the one
/// dimension of its sketch that sets the curve's diameter.
fn sketch_source(
    features: &[Arc<FeatureEntry>],
    params: &Parameters,
    frame: &dyn Fn(FeatureUid) -> Option<SketchFrame>,
    profiles: &[ProfileRef],
    segment: &SegmentKey,
    cylinder: &Cylinder,
    needed: f64,
) -> Result<FitSource, String> {
    let CurveId::Curve(curve) = &segment.curve else {
        return Err("the face comes from a text or a face's edge, not from a sketch curve".into());
    };
    let curve = *curve;
    // The sketch of the profiles that has the curve.
    let mut sketches: Vec<(&FeatureEntry, &SketchDef)> = Vec::new();
    for profile in profiles {
        let Some(entry) = features.iter().find(|f| f.uid == profile.sketch) else {
            continue;
        };
        let FeatureDef::Sketch(def) = &entry.def else {
            continue;
        };
        if def.entities.iter().any(|e| e.id == curve)
            && !sketches.iter().any(|(s, _)| s.uid == entry.uid)
        {
            sketches.push((&**entry, def));
        }
    }
    let (sketch, def) = match sketches.as_slice() {
        [one] => *one,
        [] => return Err("the face's curve is in none of the feature's sketches".to_owned()),
        _ => return Err("the face's curve is in several sketches".to_owned()),
    };
    let name = |id: ParamId| params.name(id);
    let value_of = |id: ParamId| {
        params
            .value(id)
            .ok_or_else(|| format!("no value of {}", params.name(id)))
    };
    // The axis in the sketch's plane, for a line turned about it.
    let axis = frame(sketch.uid).and_then(|f| axis_in(&f, cylinder));
    let measure = |solved: &Solved| -> Option<f64> { diameter_of(solved, curve, axis) };
    let solve = |moved: Option<(ParamId, f64)>| -> Option<f64> {
        let solved = def
            .solve_with(
                &mut |id| match moved {
                    Some((m, v)) if m == id => Ok(v),
                    _ => value_of(id),
                },
                &name,
            )
            .ok()?;
        measure(&solved)
    };
    let Some(now) = solve(None) else {
        return Err(format!(
            "{} does not give the face's diameter (it does not solve, or the curve is not \
             round or along the axis)",
            sketch.name
        ));
    };
    if (now - 2.0 * cylinder.radius).abs() > MATCH {
        return Err(format!(
            "the face is no longer as {} draws it (moved or changed after)",
            sketch.name
        ));
    }
    // The driving dimensions that move it.
    let mut movers: Vec<(ParamId, f64, f64)> = Vec::new();
    for dimension in def.dimensions.iter().filter(|d| d.active) {
        let Some(id) = dimension.value else {
            continue;
        };
        let Ok(value) = value_of(id) else {
            continue;
        };
        // (A step large against the solver's noise.)
        let step = 1e-2 * value.abs().max(1.0);
        let Some(moved) = solve(Some((id, value + step))) else {
            continue;
        };
        if (moved - now).abs() > MOVES * step {
            movers.push((id, value, (moved - now) / step));
        }
    }
    let (id, value, slope) = match movers.as_slice() {
        [one] => *one,
        // A projected curve (mitcad#1780): it follows the model geometry
        // it was projected from, which no dimension of the sketch moves.
        [] if is_projected(def, curve) => {
            return Err(format!(
                "the face follows an edge projected into {}: draw and dimension the curve \
                 instead, or change the geometry it was projected from",
                sketch.name
            ));
        }
        [] => {
            return Err(format!(
                "no dimension of {} sets the face's diameter",
                sketch.name
            ));
        }
        _ => {
            return Err(format!(
                "the face's diameter comes from several dimensions of {}",
                sketch.name
            ));
        }
    };
    // The value that gives the crest diameter: from the face's diameter as
    // the kernel measured it (the sketch's own solves here may lie some
    // 1e-5 mm off it) along the slope, one of SLOPES where it is that, so
    // that no solver noise reaches the value (72/2 becomes exactly
    // 73.835/2); checked by a solve; then the secant until it is reached.
    let slope = SLOPES
        .iter()
        .copied()
        .find(|s| (slope - s).abs() <= SLOPE_MATCH)
        .unwrap_or(slope);
    let diameter = 2.0 * cylinder.radius;
    let (mut a, mut fa) = (value, diameter);
    let mut b = value + (needed - diameter) / slope;
    for _ in 0..8 {
        let Some(fb) = solve(Some((id, b))) else {
            break;
        };
        if (fb - needed).abs() <= REACH {
            if b <= 0.0 && value > 0.0 {
                return Err(format!(
                    "{}'s dimension would have to be 0 or less",
                    sketch.name
                ));
            }
            return Ok(FitSource {
                feature: sketch.uid,
                parameter: id.raw(),
                value: b,
            });
        }
        if (fb - fa).abs() < f64::EPSILON {
            break;
        }
        let next = b + (needed - fb) * (b - a) / (fb - fa);
        (a, fa, b) = (b, fb, next);
    }
    Err(format!(
        "{}'s dimension cannot give the face the diameter the thread needs",
        sketch.name
    ))
}

/// A curve of the sketch that is projected model geometry (mitcad#1780): a
/// reference entity (a projection, linked or not, or included geometry) or
/// one a linked projection made.
fn is_projected(def: &SketchDef, curve: EntityUid) -> bool {
    def.entities.iter().any(|e| e.id == curve && e.reference)
        || def
            .projections
            .iter()
            .any(|p| p.entities.iter().any(|r| r.uid() == curve))
}

/// A cylinder's axis in a sketch's plane: a point and a unit direction in
/// sketch coordinates; None when it does not lie in the plane.
fn axis_in(frame: &SketchFrame, cylinder: &Cylinder) -> Option<Axis2> {
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let o = cylinder.axis.origin;
    let d = cylinder.axis.direction;
    let rel = [
        o[0] - frame.origin[0],
        o[1] - frame.origin[1],
        o[2] - frame.origin[2],
    ];
    let point = [dot(rel, frame.x_axis), dot(rel, frame.y_axis)];
    let along = [dot(d, frame.x_axis), dot(d, frame.y_axis)];
    let length = along[0].hypot(along[1]);
    let off = dot(rel, mitcad_vec3::cross(frame.x_axis, frame.y_axis));
    if length < 1.0 - 1e-6 || off.abs() > 1e-6 * point[0].abs().max(point[1].abs()).max(1.0) {
        return None;
    }
    Some((point, [along[0] / length, along[1] / length]))
}

/// The diameter a solved curve gives its face: a circle's or an arc's, or
/// twice a line's distance from `axis` when it runs along it.
fn diameter_of(solved: &Solved, curve: EntityUid, axis: Option<Axis2>) -> Option<f64> {
    match solved.curves.get(&curve)? {
        Curve2::Circle { radius, .. } | Curve2::Arc { radius, .. } => Some(2.0 * radius),
        Curve2::Line { a, b } => {
            let (p, d) = axis?;
            let run = [b[0] - a[0], b[1] - a[1]];
            let length = run[0].hypot(run[1]);
            if length <= 0.0 || (run[0] * d[1] - run[1] * d[0]).abs() > 1e-6 * length {
                return None;
            }
            let m = [(a[0] + b[0]) / 2.0 - p[0], (a[1] + b[1]) / 2.0 - p[1]];
            Some(2.0 * (m[0] * d[1] - m[1] * d[0]).abs())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hole_walls_are_told_by_their_role() {
        assert!(is_hole_wall("hole0.wall"));
        assert!(is_hole_wall("hole12.wall"));
        assert!(!is_hole_wall("hole.wall"));
        assert!(!is_hole_wall("hole0.cbore_wall"));
        assert!(!is_hole_wall("side"));
    }

    #[test]
    fn a_line_along_the_axis_gives_twice_its_distance() {
        let mut solved = Solved::default();
        solved.curves.insert(
            EntityUid(3),
            Curve2::Line {
                a: [36.0, 60.0],
                b: [36.0, 200.0],
            },
        );
        solved.curves.insert(
            EntityUid(4),
            Curve2::Circle {
                center: [1.0, 2.0],
                radius: 4.5,
                start: 0.0,
            },
        );
        let axis = Some(([0.0, 0.0], [0.0, 1.0]));
        assert_eq!(diameter_of(&solved, EntityUid(3), axis), Some(72.0));
        assert_eq!(diameter_of(&solved, EntityUid(4), None), Some(9.0));
        // Across the axis: no cylinder.
        assert_eq!(
            diameter_of(&solved, EntityUid(3), Some(([0.0, 0.0], [1.0, 0.0]))),
            None
        );
    }

    #[test]
    fn an_axis_in_the_sketch_plane_is_found_there() {
        // The plane of the xz origin plane's kind: x along x, y along z.
        let frame = SketchFrame {
            origin: [0.0, 0.0, 0.0],
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 0.0, 1.0],
        };
        let cylinder = Cylinder {
            axis: crate::kernel::Axis {
                origin: [0.0, 0.0, 60.0],
                direction: [0.0, 0.0, 1.0],
            },
            radius: 36.0,
            length: 10.0,
            internal: true,
        };
        assert_eq!(axis_in(&frame, &cylinder), Some(([0.0, 60.0], [0.0, 1.0])));
        let off = Cylinder {
            axis: crate::kernel::Axis {
                origin: [0.0, 5.0, 60.0],
                direction: [0.0, 0.0, 1.0],
            },
            ..cylinder
        };
        assert_eq!(axis_in(&frame, &off), None);
    }
}
