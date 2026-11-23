// SPDX-License-Identifier: MIT
//! Edge frame (mitcad#1814; `commands.md`, "BUILD: timber construction",
//! *Members along an edge*): two frames along a straight edge of a body
//! lying on a planar face beside it, named `EdgeFrame<n>`. BUILD's Timber
//! places a member along an edge on them: the member's joint is on the
//! `start` frame and its bound length runs from `start` to the plane of
//! the `end` frame, so that the member follows the edge and the face when
//! the body changes (its length, width or thickness).
//!
//! The edge and the face are geometry of the component the occurrence path
//! ends in (empty: the feature's own component), taken into the feature's
//! component through the occurrences' placements at this point of the
//! timeline, as a bound length's are. Both frames have their z axis along
//! the edge, from its start to its end, their x axis across the face away
//! from the edge (into the face) and their y axis the face's outward normal.
//! The edge's start is the end that makes them right-handed: looking along
//! the edge with the face's normal up, the face lies on the left. The edge
//! is projected onto the face's plane.
//!
//! - `inset`: across the face, away from the edge (0: at the edge, negative
//!   out over it);
//! - `start`, `end`: along the edge from each of its ends (positive
//!   shortens, negative runs past the end);
//! - `lift`: off the face along its normal (0: on it).
//!
//! A curved edge, a face that is not planar, an edge that does not run
//! along the face and offsets that leave nothing of the edge fail the
//! feature in words; a lost reference fails it as other features' lost
//! references do.
//!
//! The edge may be a sketch line instead (mitcad#1965, BUILD's Profile
//! *Along sketch lines*): a line of a planar sketch or of a 3D sketch,
//! with no face. The frames then run along the line from its start to its
//! end (`reversed`: from its end to its start), their y axis the planar
//! sketch's normal (a 3D sketch's line: the component's Z square to the
//! line, its Y for a line along Z) and their x axis `y × z`, to the left
//! of the line looking along it with y up. `inset` and `lift` move them
//! along x and y. An arc, a circle or a spline of the sketch fails the
//! feature in words: members go along straight lines only.
//!
//! Along a sketch line, each end may be in a joint with another member
//! along a line (mitcad#2200, BUILD's joint conditions; `start_meets`,
//! `end_meets`, [`EdgeMeet`]): the end is worked out at every evaluation
//! from the other member's line and the two sections kept with the joint
//! (to the other's near side, through to its far side, or to the outer
//! corner of a shared miter, [`meet_offset`]), and the `start` or `end`
//! offset adds to it. So the end follows when either line moves, and the
//! member still follows its own line. A shared miter's cut angle is
//! measured from the lines too (`start_angle`, `end_angle`, reference
//! parameters of the feature's own, [`mitre_angle`]; mitcad#2234), which the
//! member's derived part takes as its end's cut angle, so the cut follows
//! when the corner's angle changes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::bound_length::{InComponent, path_placement};
use super::geom_ref::{GeomRef, Resolver, Want};
use super::joint::OccurrencePath;
use super::mapping::map_params;
use super::pattern::none;
use super::{CheckContext, EvalContext, Evaluate, FeatureInfo, FeatureOutput, References};
use crate::datum::{CurveGeometry, SurfaceGeometry, Vec3};
use crate::ids::{EntityUid, FeatureUid};
use crate::kernel::Curve3;
use crate::kernel::Kernel;
use crate::notice::{NoticePlace, about_place, placed, said};
use crate::parameters::ParamId;
use crate::sketch::geometry::Curve2;
use crate::transform::Transform;
use mitcad_vec3::{add, cross, dot, norm, scale, sub, unit};

/// The name of the frame at the edge's start (after `start`).
pub const START: &str = "start";
/// The name of the frame at the edge's end (before `end`).
pub const END: &str = "end";
/// The names of an edge frame's mount frames.
pub const MOUNTS: [&str; 2] = [START, END];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeFrameDef<P = ParamId> {
    /// A path from the feature's component to the component the edge and
    /// the face are in; empty for its own.
    #[serde(default, skip_serializing_if = "OccurrencePath::is_empty")]
    pub occurrence: OccurrencePath,
    /// A straight edge, or a sketch line (mitcad#1965).
    pub edge: GeomRef,
    /// The planar face the frames lie on; none along a sketch line, whose
    /// sketch gives the plane (mitcad#1965).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<GeomRef>,
    /// Across the face, away from the edge, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub inset: Option<P>,
    /// Along the edge from its start, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub start: Option<P>,
    /// Along the edge from its end, back towards the start, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub end: Option<P>,
    /// Off the face along its normal, mm.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub lift: Option<P>,
    /// Along a sketch line: from its end to its start (mitcad#1965).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reversed: bool,
    /// Along a sketch line: how the member BUILD placed on these frames
    /// sits on the line (`center`, `above`, …), kept for its edits; the
    /// frames do not use it (mitcad#1965).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    /// Along a sketch line: the joint the member's end at `start` (its end
    /// A) is in (mitcad#2200): `start` is worked out from the other
    /// member's line at every evaluation, the `start` offset added to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_meets: Option<EdgeMeet>,
    /// Likewise at `end` (the member's end B).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_meets: Option<EdgeMeet>,
    /// Along a sketch line, a shared miter at `start` (`start_meets` to
    /// `corner`): the miter's cut angle, measured from both lines at every
    /// evaluation into this reference parameter of the feature's own, which
    /// the member's derived part takes as its end A's cut angle
    /// (mitcad#2234).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub start_angle: Option<P>,
    /// Likewise at `end` (the member's end B).
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub end_angle: Option<P>,
}

/// Where a member's end along a sketch line goes in a joint with another
/// member along a line (mitcad#2200; BUILD's joint conditions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MeetSide {
    /// To the other member's near side: a T's stem, or an L's member that
    /// stops at the side of the one running through.
    NearSide,
    /// Through to the other member's far (outer) side: a T's stem through
    /// it, or an L's member running on to the outer corner.
    FarSide,
    /// A shared miter's long point: to where the outer sides of both
    /// members cross.
    Corner,
}

/// A member's end in a joint with another member along a sketch line
/// (mitcad#2200): the other member's line and the sections of both, across
/// their lines. A section is `[x min, x max, y min, y max]` in mm, in the
/// frame of its line as it runs from its start to its end (`reversed` left
/// out; see the module for x and y), taken from the member as placed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EdgeMeet {
    pub to: MeetSide,
    /// The other member's sketch line, in the same component as `edge`.
    pub edge: GeomRef,
    /// The other member's section across its line.
    pub section: [f64; 4],
    /// This member's own section across its line.
    pub own: [f64; 4],
}

impl<P> EdgeFrameDef<P> {
    pub const TYPE: &'static str = "edge_frame";
    pub const BASE_NAME: &'static str = "EdgeFrame";

    map_params! {
        EdgeFrameDef {
            occurrence: clone,
            edge: clone,
            face: clone,
            inset: option,
            start: option,
            end: option,
            lift: option,
            reversed: clone,
            align: clone,
            start_meets: clone,
            end_meets: clone,
            start_angle: option,
            end_angle: option,
        }
    }

    /// The joints at its two ends (`start_meets`, `end_meets`), with whether
    /// each is at `end`.
    pub fn meets(&self) -> impl Iterator<Item = (bool, &EdgeMeet)> {
        [(false, &self.start_meets), (true, &self.end_meets)]
            .into_iter()
            .filter_map(|(end, meet)| meet.as_ref().map(|m| (end, m)))
    }
}

impl FeatureInfo for EdgeFrameDef {
    fn references(&self) -> References {
        let mut references = References::default();
        self.edge.add_to(&mut references);
        if let Some(face) = &self.face {
            face.add_to(&mut references);
        }
        for (_, meet) in self.meets() {
            meet.edge.add_to(&mut references);
        }
        references
    }

    fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match &self.edge {
            GeomRef::Edge { .. } => self
                .edge
                .check(ctx, Want::Line)
                .map_err(|e| format!("edge: {e}"))?,
            // A sketch line (mitcad#1965).
            GeomRef::SketchCurves { curves, .. } if curves.len() == 1 => self
                .edge
                .check(ctx, Want::Line)
                .map_err(|e| format!("edge: {e}"))?,
            other => {
                return Err(said(
                    "an edge frame runs along an edge or a sketch line: pick a straight edge or one line",
                    format!("edge: {other} is no edge and no sketch line"),
                ));
            }
        }
        match &self.face {
            Some(face @ GeomRef::Face { .. }) => face
                .check(ctx, Want::Face)
                .map_err(|e| format!("face: {e}"))?,
            Some(other) => {
                return Err(said(
                    "an edge frame lies on a face: pick the flat face beside the edge",
                    format!("face: {other} is no face"),
                ));
            }
            None if matches!(self.edge, GeomRef::Edge { .. }) => {
                return Err(said(
                    "an edge frame along a body's edge lies on a face: pick the flat face beside the edge",
                    "face: missing for an edge".to_owned(),
                ));
            }
            None => {}
        }
        // A joint with another member's line (mitcad#2200): along a sketch
        // line, with another line.
        for (_, meet) in self.meets() {
            if !matches!(self.edge, GeomRef::SketchCurves { .. }) {
                return Err(said(
                    "a joint with another member is kept along sketch lines only",
                    "start_meets/end_meets: the edge is no sketch line".to_owned(),
                ));
            }
            match &meet.edge {
                GeomRef::SketchCurves { curves, .. } if curves.len() == 1 => meet
                    .edge
                    .check(ctx, Want::Line)
                    .map_err(|e| format!("meets: {e}"))?,
                other => {
                    return Err(said(
                        "a joint meets another member's sketch line: pick one line",
                        format!("meets: {other} is no sketch line"),
                    ));
                }
            }
            if meet.section.iter().chain(&meet.own).any(|v| !v.is_finite()) {
                return Err(said(
                    "a joint's sections are numbers in mm",
                    "meets: a section is not finite".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// The offsets of an edge frame, in mm, and its direction along a sketch
/// line.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EdgeOffsets {
    pub inset: f64,
    pub start: f64,
    pub end: f64,
    pub lift: f64,
    /// Along a sketch line: from its end to its start (mitcad#1965).
    pub reversed: bool,
}

/// An edge frame worked out: its two frames (see the module) in the
/// coordinates of the component the edge and the face are in, and their
/// distance along the edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeRun {
    pub start: Transform,
    pub end: Transform,
    pub length: f64,
    /// The whole edge's length along it.
    pub edge_length: f64,
}

/// What a sketch curve is to a member along it (mitcad#1965): a straight
/// line from its start to its end, with the planar sketch's normal (none
/// for a 3D sketch's line), or a curve that is not straight.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SketchLine {
    Line {
        start: Vec3,
        end: Vec3,
        normal: Option<Vec3>,
    },
    /// An arc, a circle, an ellipse, a spline or a point: what it is, in
    /// words.
    Curved(&'static str),
}

/// A curve of a sketch (planar or 3D) as a member along it sees it
/// (mitcad#1965), in its component's coordinates.
pub fn sketch_line<R: Resolver>(
    r: &mut R,
    sketch: FeatureUid,
    curve: EntityUid,
) -> Result<SketchLine, String> {
    // A 3D sketch's curve.
    if let Some(found) = r.spatial_entity(sketch, crate::sketch::Ref::Curve(curve)) {
        return Ok(match found? {
            Curve3::Line { start, end } => SketchLine::Line {
                start,
                end,
                normal: None,
            },
            Curve3::Conic { .. } => SketchLine::Curved("an arc or a circle"),
            Curve3::Point(_) => SketchLine::Curved("a point"),
            _ => SketchLine::Curved("a spline"),
        });
    }
    let output = r.sketch(sketch)?;
    let frame = output.frame;
    match output.solved.curves.get(&curve) {
        Some(Curve2::Line { a, b }) => Ok(SketchLine::Line {
            start: frame.point(*a),
            end: frame.point(*b),
            normal: unit(frame.normal()),
        }),
        Some(Curve2::Circle { .. } | Curve2::Arc { .. }) => {
            Ok(SketchLine::Curved("an arc or a circle"))
        }
        Some(Curve2::Ellipse { .. }) => Ok(SketchLine::Curved("an ellipse")),
        Some(_) => Ok(SketchLine::Curved("a spline")),
        None => Err(said(
            "the sketch has no such line: pick the line again",
            format!("{} has no curve {}", r.name(sketch), curve.curve_name()),
        )),
    }
}

/// A sketch line's frame as it runs from its start to its end (`reversed`
/// left out), in its component's coordinates: z along it, y (`n`) up (the
/// sketch's normal square to it; a 3D sketch's line: Z square to it, Y for
/// a line along Z) and x `y × z`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineFrame {
    pub start: Vec3,
    pub end: Vec3,
    pub x: Vec3,
    pub n: Vec3,
    pub z: Vec3,
}

impl LineFrame {
    /// How far a section (`[x min, x max, y min, y max]` across this line)
    /// reaches from the line towards `d`, a direction square to it.
    pub fn reach(&self, section: &[f64; 4], d: Vec3) -> f64 {
        let (dx, dy) = (dot(d, self.x), dot(d, self.n));
        [
            (section[0], section[2]),
            (section[1], section[2]),
            (section[0], section[3]),
            (section[1], section[3]),
        ]
        .iter()
        .map(|(x, y)| x * dx + y * dy)
        .fold(f64::NEG_INFINITY, f64::max)
    }

    /// The section's middle across the line, as a point beside `p` on it.
    pub fn middle(&self, section: &[f64; 4], p: Vec3) -> Vec3 {
        add(
            p,
            add(
                scale(self.x, (section[0] + section[1]) / 2.0),
                scale(self.n, (section[2] + section[3]) / 2.0),
            ),
        )
    }
}

/// The frame of a sketch line (`edge`, one line of a sketch) as
/// [`LineFrame`] says; a curve that is not straight fails in words.
pub fn line_frame<R: Resolver>(r: &mut R, edge: &GeomRef) -> Result<LineFrame, String> {
    let GeomRef::SketchCurves { sketch, curves } = edge else {
        return Err(said(
            "a member along a sketch line needs one line: pick one",
            format!("edge: {edge} is no sketch line"),
        ));
    };
    let [curve] = curves.as_slice() else {
        return Err(said(
            "a member along a sketch line needs one line: pick one",
            format!("edge: {edge} is not one sketch curve"),
        ));
    };
    let (start, end, normal) = match sketch_line(r, *sketch, *curve)? {
        SketchLine::Line { start, end, normal } => (start, end, normal),
        SketchLine::Curved(what) => {
            return Err(said(
                format!("the sketch curve is {what}: members go along straight lines only"),
                format!("{edge} is not straight"),
            ));
        }
    };
    let z = unit(sub(end, start)).ok_or_else(|| {
        placed(
            "the sketch line has no length",
            NoticePlace::point(start),
            format!("{edge} has no length"),
        )
    })?;
    // Up: the sketch's normal; a 3D sketch's line: Z square to it, Y for a
    // line along Z.
    let up = normal.unwrap_or_else(|| {
        if dot(z, [0.0, 0.0, 1.0]).abs() < 1.0 - 1e-6 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 1.0, 0.0]
        }
    });
    let n = unit(sub(up, scale(z, dot(up, z)))).ok_or_else(|| {
        placed(
            "the sketch line runs along the sketch's normal",
            NoticePlace::point(start),
            format!("{edge} is square to its sketch"),
        )
    })?;
    Ok(LineFrame {
        start,
        end,
        x: cross(n, z),
        n,
        z,
    })
}

/// Where a member's end along the line `own` goes in a joint (mitcad#2200),
/// as an offset along the line from the line's end there (positive
/// shortens, as an edge frame's `start` and `end`): `at_end` is the
/// frame's `end` (else its `start`), `reversed` the edge frame's.
///
/// - `near_side`, `far_side`: the end's section middle on the other
///   member's side facing it, or on its far side; the sides lie at the
///   other section's reach from its line, across it towards this member's
///   body and away from it.
/// - `corner` (a shared miter's long point): the end square to its line
///   through the point where the outer sides of both members cross; the
///   other member's end there is the one of its line nearer this end.
///
/// A line that runs along the other one fails in words.
pub fn meet_offset<R: Resolver>(
    r: &mut R,
    own: &GeomRef,
    reversed: bool,
    at_end: bool,
    meet: &EdgeMeet,
) -> Result<f64, String> {
    let mine = line_frame(r, own)?;
    let other = line_frame(r, &meet.edge)?;
    let line_end = at_end != reversed;
    // The line's end at this end of the member, and the way out of it.
    let (point, out) = if line_end {
        (mine.end, mine.z)
    } else {
        (mine.start, scale(mine.z, -1.0))
    };
    let along = || {
        said(
            "the member runs along the one it meets: its end is left as drawn",
            format!("{own} is parallel to {}", meet.edge),
        )
    };
    let moved = match meet.to {
        MeetSide::NearSide | MeetSide::FarSide => {
            // Square to the other line, towards this member's body.
            let square = sub(out, scale(other.z, dot(out, other.z)));
            let sin = norm(square);
            if sin < 1e-6 {
                return Err(along());
            }
            let towards = scale(square, -1.0 / sin);
            let side = match meet.to {
                MeetSide::NearSide => other.reach(&meet.section, towards),
                _ => -other.reach(&meet.section, scale(towards, -1.0)),
            };
            let middle = mine.middle(&meet.own, point);
            (dot(sub(middle, other.start), towards) - side) / sin
        }
        MeetSide::Corner => {
            let v = way_out(&other, point);
            let c = dot(out, v);
            let sin2 = 1.0 - c * c;
            if sin2 < 1e-12 {
                return Err(along());
            }
            // Across each member away from the other's body: its outer
            // side.
            let (Some(e_mine), Some(e_other)) =
                (unit(sub(v, scale(out, c))), unit(sub(out, scale(v, c))))
            else {
                return Err(along());
            };
            let h_mine = mine.reach(&meet.own, e_mine);
            let h_other = other.reach(&meet.section, e_other);
            (h_other + h_mine * c - dot(sub(point, other.start), e_other)) / sin2.sqrt()
        }
    };
    // Moved out by `moved`: an offset of as much the other way.
    Ok(-moved)
}

/// The other member's end at a corner (`point`, this member's line's end
/// there): of its line's ends the nearer, its way out of the member.
fn way_out(other: &LineFrame, point: Vec3) -> Vec3 {
    if norm(sub(other.end, point)) <= norm(sub(other.start, point)) {
        other.z
    } else {
        scale(other.z, -1.0)
    }
}

/// A shared miter's cut angle at a member's end along the line `own`
/// (mitcad#2234), from square in radians: half the turn from this
/// member's way out of the corner into the other member's line (half of
/// 180 degrees less the angle between the lines), so both members are cut
/// on the corner's bisector. `at_end`, `reversed` and `meet` as
/// [`meet_offset`]'s; lines that run along each other fail in words.
pub fn mitre_angle<R: Resolver>(
    r: &mut R,
    own: &GeomRef,
    reversed: bool,
    at_end: bool,
    meet: &EdgeMeet,
) -> Result<f64, String> {
    let (_, out, into) = mitre_axes(r, own, reversed, at_end, meet)?;
    Ok(norm(cross(out, into)).atan2(dot(out, into)) / 2.0)
}

/// The two axes that determine a shared miter. The other member's inward
/// axis also tells which way this cut leans (mitcad#2259).
pub(crate) fn mitre_axes<R: Resolver>(
    r: &mut R,
    own: &GeomRef,
    reversed: bool,
    at_end: bool,
    meet: &EdgeMeet,
) -> Result<(LineFrame, Vec3, Vec3), String> {
    let mine = line_frame(r, own)?;
    let other = line_frame(r, &meet.edge)?;
    let (point, out) = if at_end != reversed {
        (mine.end, mine.z)
    } else {
        (mine.start, scale(mine.z, -1.0))
    };
    let into = scale(way_out(&other, point), -1.0);
    if norm(cross(out, into)) < 1e-6 {
        return Err(said(
            "the member runs along the one it meets: its end is left as drawn",
            format!("{own} is parallel to {}", meet.edge),
        ));
    }
    Ok((mine, out, into))
}

/// An edge frame's miter angles are its own references (mitcad#2234): the
/// number a new one is given makes a dimension of the feature, which
/// becomes the reference, as a bound length's parameter does.
pub(crate) fn settle_angles(
    params: &mut crate::parameters::Parameters,
    def: &EdgeFrameDef,
    owner: FeatureUid,
) -> Result<(), String> {
    for (slot, id) in [
        ("start_angle", def.start_angle),
        ("end_angle", def.end_angle),
    ] {
        let Some(id) = id else {
            continue;
        };
        if params.owner(id) != Some(owner) {
            return Err(format!(
                "{slot}: an edge frame measures a miter's angle in a parameter of its own; give \
                 its first value as a number, not “{}”",
                params.name(id)
            ));
        }
        params.set_kind(id, crate::expr::ParamKind::Reference);
    }
    Ok(())
}

/// Works out an edge frame's frames from a straight edge and a planar face,
/// or from a sketch line (see the module), in the coordinates of their
/// component. Shared by the feature's evaluation and BUILD's planning, so
/// that a preview is where the placed member goes.
pub fn edge_run<R: Resolver>(
    r: &mut R,
    edge: &GeomRef,
    face: Option<&GeomRef>,
    offsets: &EdgeOffsets,
) -> Result<EdgeRun, String> {
    // Along a sketch line (mitcad#1965).
    if matches!(edge, GeomRef::SketchCurves { .. }) {
        let line = line_frame(r, edge)?;
        let (p0, p1, x, z) = if offsets.reversed {
            (
                line.end,
                line.start,
                scale(line.x, -1.0),
                scale(line.z, -1.0),
            )
        } else {
            (line.start, line.end, line.x, line.z)
        };
        return run_between(p0, p1, x, line.n, z, offsets);
    }
    let Some(face) = face else {
        return Err(said(
            "a member along an edge lies on a face: pick the flat face beside the edge",
            "face: missing".to_owned(),
        ));
    };
    let GeomRef::Face {
        body: face_body,
        face: face_name,
    } = face
    else {
        return Err(said(
            "a member along an edge lies on a face: pick the flat face beside the edge",
            format!("face: {face} is no face"),
        ));
    };
    let GeomRef::Edge {
        body: edge_body,
        edge: edge_name,
    } = edge
    else {
        return Err(said(
            "a member along an edge needs a straight edge: pick one",
            format!("edge: {edge} is no edge"),
        ));
    };
    // The face's plane and middle.
    let shape = r.shape(*face_body)?;
    let found_face = crate::features::renamed::face_for(r, &shape, face_name)?;
    let face_place = || NoticePlace::face(found_face.to_string()).in_body(&face_body.to_string());
    let (origin, normal) = match r.kernel().face_geometry(&shape, &found_face) {
        Ok(SurfaceGeometry::Plane { origin, normal }) => (origin, normal),
        Ok(other) => {
            return Err(placed(
                "the face is not flat: a member along an edge lies on a flat face",
                face_place(),
                format!("face: {found_face} is not planar ({})", other.kind()),
            ));
        }
        // A kernel that tells only the plane of a face.
        Err(e) => match r.kernel().face_plane(&shape, &found_face) {
            Ok(plane) => (plane.origin, plane.normal),
            Err(_) => return Err(about_place(face_place(), &e.to_string())),
        },
    };
    let middle = r
        .kernel()
        .face_plane(&shape, &found_face)
        .map_or(origin, |p| p.origin);
    let n = unit(normal).ok_or_else(|| {
        placed(
            "the face has no normal",
            face_place(),
            format!("face: {found_face} has no normal"),
        )
    })?;
    // The edge, straight.
    let shape = r.shape(*edge_body)?;
    let found_edge = crate::features::renamed::edge_for(r, &shape, edge_name)?;
    let edge_place = || NoticePlace::edge(found_edge.to_string()).in_body(&edge_body.to_string());
    let (a, b) = match r.kernel().edge_geometry(&shape, &found_edge) {
        Ok(CurveGeometry::Line { start, end }) => (start, end),
        Ok(_) => {
            return Err(placed(
                "the edge is curved: a member along an edge needs a straight edge",
                edge_place(),
                format!("edge: {found_edge} is not straight"),
            ));
        }
        Err(e) => return Err(about_place(edge_place(), &e.to_string())),
    };
    let along = unit(sub(b, a)).ok_or_else(|| {
        placed(
            "the edge has no length",
            edge_place(),
            format!("edge: {found_edge} has no length"),
        )
    })?;
    if dot(along, n).abs() > 1e-6 {
        return Err(placed(
            "the edge does not run along the face: pick an edge of the face's plane",
            edge_place(),
            format!("edge: {found_edge} is not parallel to face {found_face}"),
        ));
    }
    // Onto the face's plane.
    let onto = |p: Vec3| sub(p, scale(n, dot(sub(p, origin), n)));
    let (a, b) = (onto(a), onto(b));
    // The face lies on the left looking from the start along the edge, its
    // normal up: x (into the face) = n × z.
    let mut z = along;
    let mut x = cross(n, z);
    let (mut p0, mut p1) = (a, b);
    let mid = scale(add(a, b), 0.5);
    if dot(x, sub(middle, mid)) < 0.0 {
        z = scale(z, -1.0);
        x = scale(x, -1.0);
        std::mem::swap(&mut p0, &mut p1);
    }
    run_between(p0, p1, x, n, z, offsets)
}

/// The two frames between `p0` and `p1` (along the unit `z`), `x` across
/// and `n` up, moved by the offsets.
fn run_between(
    p0: Vec3,
    p1: Vec3,
    x: Vec3,
    n: Vec3,
    z: Vec3,
    offsets: &EdgeOffsets,
) -> Result<EdgeRun, String> {
    let edge_length = norm(sub(p1, p0));
    let length = edge_length - offsets.start - offsets.end;
    if length.is_nan() || length <= 1e-6 {
        return Err(said(
            "the offsets at the ends leave nothing of the edge: make Start and End shorter",
            format!(
                "the edge is {} mm long; start {} and end {} leave {} mm",
                crate::build::round(edge_length),
                crate::build::round(offsets.start),
                crate::build::round(offsets.end),
                crate::build::round(length)
            ),
        ));
    }
    let across = add(scale(x, offsets.inset), scale(n, offsets.lift));
    let start = crate::joints::frame(add(add(p0, scale(z, offsets.start)), across), x, z);
    let end = crate::joints::frame(add(sub(p1, scale(z, offsets.end)), across), x, z);
    Ok(EdgeRun {
        start,
        end,
        length,
        edge_length,
    })
}

impl EdgeFrameDef {
    /// The offsets' values.
    fn offsets<K: Kernel>(&self, ctx: &mut EvalContext<'_, K>) -> Result<EdgeOffsets, String> {
        let mut value = |id: Option<ParamId>| match id {
            Some(id) => ctx.param(id),
            None => Ok(0.0),
        };
        Ok(EdgeOffsets {
            inset: value(self.inset)?,
            start: value(self.start)?,
            end: value(self.end)?,
            lift: value(self.lift)?,
            reversed: self.reversed,
        })
    }
}

impl<K: Kernel> Evaluate<K> for EdgeFrameDef {
    fn evaluate(&self, ctx: &mut EvalContext<'_, K>) -> Result<FeatureOutput<K::Shape>, String> {
        let mut offsets = self.offsets(ctx)?;
        let (at, placement) = path_placement(ctx, &self.occurrence)?;
        let mut measured = Vec::new();
        let run = {
            let mut resolver = InComponent {
                ctx: &mut *ctx,
                component: at,
            };
            // The ends in joints with other members (mitcad#2200), from
            // their lines now; the offsets add to them.
            for (at_end, meet) in self.meets() {
                let by = meet_offset(&mut resolver, &self.edge, self.reversed, at_end, meet)?;
                if at_end {
                    offsets.end += by;
                } else {
                    offsets.start += by;
                }
                // A shared miter's cut angle, from the lines now
                // (mitcad#2234).
                let angle = if at_end {
                    self.end_angle
                } else {
                    self.start_angle
                };
                if let (MeetSide::Corner, Some(id)) = (meet.to, angle) {
                    let value =
                        mitre_angle(&mut resolver, &self.edge, self.reversed, at_end, meet)?;
                    measured.push((id, value));
                }
            }
            edge_run(&mut resolver, &self.edge, self.face.as_ref(), &offsets)?
        };
        let mut mounts = BTreeMap::new();
        mounts.insert(START.to_owned(), placement.after(&run.start));
        mounts.insert(END.to_owned(), placement.after(&run.end));
        Ok(FeatureOutput {
            mounts,
            measured,
            ..FeatureOutput::default()
        })
    }
}
