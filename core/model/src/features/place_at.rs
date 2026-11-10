// SPDX-License-Identifier: MIT
//! Placement by picked geometry (mitcad#656): where a part goes, kept as
//! the references the user picked, never as coordinates. One placement
//! ([`PlaceAt`], the `at` of pulleys, gears, bearings, coils, primitives
//! and holes) has three forms:
//!
//! - `axis`: a shaft or a hole (a circular edge or a cylindrical face,
//!   resolved as a seat is: `seat.rs`), or an axis on its own (an origin
//!   or construction axis, a sketch line or circle, a fixed axis), which
//!   has no diameter;
//! - `point`: a sketch point, a sketch circle's or arc's centre, a vertex,
//!   a construction point or a circular edge's centre, the axis the normal
//!   of its sketch, of its circle or of `plane`;
//! - `face`: a point in a planar face's (or a plane's) plane at `offset1`
//!   and `offset2` from the lines of two edges, or on an origin plane from
//!   its origin axes.
//!
//! Each resolves into a frame ([`Placed`]): the origin the part is placed
//! at, the normal it runs along and an x axis. `flip`, `offset`, `align`
//! and `angle` then turn and move it. [`EvalContext::place`] resolves a
//! placement during evaluation (recording what it reads),
//! [`Document::place_frame`] at the timeline marker (the panels' and the
//! Gear Pair and Belt Drive queries).

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Resolver, Want, resolve_axis, resolve_plane, resolve_point};
use super::seat::{Seat, SeatFrame, round_of};
use super::{CheckContext, EvalContext, References, is_false};
use crate::datum::{DatumAxis, DatumPlane, Vec3};
use crate::document::Document;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::sketch::geometry::Curve2;
use mitcad_vec3::{add, norm, scale, sub, unit};
use mitcad_vec3::{cross, dot};

fn none<P>() -> Option<P> {
    None
}

/// Where a part is placed (see the module documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaceAt<P = ParamId> {
    /// On an axis: a shaft or a hole, or an axis of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<GeomRef>,
    /// At a point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<GeomRef>,
    /// With `point`: the plane whose normal is the axis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plane: Option<GeomRef>,
    /// On a face (or a plane), at offsets from two lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<GeomRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge1: Option<GeomRef>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset1: Option<P>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge2: Option<GeomRef>,
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset2: Option<P>,
    /// Where the pick was, mm: the end of a cylindrical face measured
    /// from, the side of each line on a plane.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near: Option<Vec3>,
    /// Along the axis, mm; 0 without it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// The axis turned round.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// Turns the part about its axis, after `align`.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub angle: Option<P>,
    /// A line the part's x axis lies along, or a planar face (a plane)
    /// whose normal it lies along (a shaft's flat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<GeomRef>,
    /// With `align`: a direction the part's y axis points toward
    /// (mitcad#656). A line's own direction is reversed where its y would
    /// point away, so that a part placed from a face's far edge grows into
    /// the face. Without it a line's direction is taken as it is (designs
    /// written before open unchanged).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align_side: Option<Vec3>,
}

/// The form of a placement and its main reference.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Form<'a> {
    Axis(&'a GeomRef),
    Point(&'a GeomRef),
    Face(&'a GeomRef),
}

/// Whether a reference is a body's face or edge (a shaft or a hole).
fn on_body(reference: &GeomRef) -> bool {
    matches!(reference, GeomRef::Face { .. } | GeomRef::Edge { .. })
}

impl<P> PlaceAt<P> {
    /// Converts the value fields; their slots are `<prefix>offset1`,
    /// `<prefix>offset2`, `<prefix>offset` and `<prefix>angle` (`at.`).
    pub fn map_params<Q, E>(
        &self,
        prefix: &str,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<PlaceAt<Q>, E> {
        let mut option = |slot: &str, v: &Option<P>| {
            v.as_ref()
                .map(|v| f(&format!("{prefix}{slot}"), v))
                .transpose()
        };
        let offset1 = option("offset1", &self.offset1)?;
        let offset2 = option("offset2", &self.offset2)?;
        let offset = option("offset", &self.offset)?;
        let angle = option("angle", &self.angle)?;
        Ok(PlaceAt {
            axis: self.axis.clone(),
            point: self.point.clone(),
            plane: self.plane.clone(),
            face: self.face.clone(),
            edge1: self.edge1.clone(),
            offset1,
            edge2: self.edge2.clone(),
            offset2,
            near: self.near,
            offset,
            flip: self.flip,
            angle,
            align: self.align.clone(),
            align_side: self.align_side,
        })
    }

    /// Which form it is: exactly one of `axis`, `point` and `face`.
    pub fn form(&self) -> Result<Form<'_>, String> {
        match (&self.axis, &self.point, &self.face) {
            (Some(axis), None, None) => Ok(Form::Axis(axis)),
            (None, Some(point), None) => Ok(Form::Point(point)),
            (None, None, Some(face)) => Ok(Form::Face(face)),
            (None, None, None) => Err("a placement needs an axis, a point or a face".to_owned()),
            _ => Err("a placement takes one of an axis, a point and a face".to_owned()),
        }
    }

    /// On a body's shaft or hole: the part's bore can follow it.
    pub fn on_shaft(&self) -> bool {
        self.axis.as_ref().is_some_and(on_body)
    }

    /// Adds what the placement refers to.
    pub fn add_to(&self, references: &mut References) {
        for reference in [
            &self.axis,
            &self.point,
            &self.plane,
            &self.face,
            &self.edge1,
            &self.edge2,
            &self.align,
        ]
        .into_iter()
        .flatten()
        {
            reference.add_to(references);
        }
    }

    /// Checks the references before the feature.
    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        let form = self.form()?;
        let is_face = matches!(form, Form::Face(_));
        if self.plane.is_some() && !matches!(form, Form::Point(_)) {
            return Err("only a placement at a point takes a plane".to_owned());
        }
        if !is_face
            && (self.edge1.is_some()
                || self.edge2.is_some()
                || self.offset1.is_some()
                || self.offset2.is_some())
        {
            return Err("only a placement on a face takes edges and offsets from them".to_owned());
        }
        if self.edge1.is_some() != self.edge2.is_some() {
            return Err(
                "a placement on a face is measured from two edges (or from the origin axes on an origin plane)"
                    .to_owned(),
            );
        }
        if self.near.is_some_and(|p| p.iter().any(|v| !v.is_finite())) {
            return Err("the placement's picked point must be finite".to_owned());
        }
        match form {
            Form::Axis(axis) => {
                let want = if on_body(axis) {
                    Want::EdgeOrFace
                } else {
                    Want::Axis
                };
                axis.check(ctx, want).map_err(|e| format!("axis: {e}"))?;
            }
            Form::Point(point) => {
                let want = if matches!(point, GeomRef::SketchCurves { .. }) {
                    Want::Axis
                } else {
                    Want::Point
                };
                point.check(ctx, want).map_err(|e| format!("point: {e}"))?;
                if let Some(plane) = &self.plane {
                    plane
                        .check(ctx, Want::Plane)
                        .map_err(|e| format!("plane: {e}"))?;
                }
            }
            Form::Face(face) => {
                face.check(ctx, Want::Plane)
                    .map_err(|e| format!("face: {e}"))?;
                match (&self.edge1, &self.edge2) {
                    (Some(a), Some(b)) => {
                        a.check(ctx, Want::Line)
                            .map_err(|e| format!("edge1: {e}"))?;
                        b.check(ctx, Want::Line)
                            .map_err(|e| format!("edge2: {e}"))?;
                    }
                    _ if !matches!(face, GeomRef::Origin(_)) => {
                        return Err(
                            "a point on a face is measured from two of its edges (only an origin plane measures from its origin axes)"
                                .to_owned(),
                        );
                    }
                    _ => {}
                }
            }
        }
        if self.align_side.is_some() && self.align.is_none() {
            return Err("only a placement with align takes align_side".to_owned());
        }
        if self
            .align_side
            .is_some_and(|p| p.iter().any(|v| !v.is_finite()))
        {
            return Err("the placement's align_side must be finite".to_owned());
        }
        // A line, or a planar face or a plane (its normal; mitcad#656).
        if let Some(align) = &self.align {
            align
                .check(ctx, Want::Line)
                .or_else(|_| align.check(ctx, Want::Plane))
                .map_err(|e| format!("align: {e}"))?;
        }
        Ok(())
    }
}

/// An optional placement's values (`map_params!`'s `with`): slots `at.…`.
pub(crate) fn map_at<P, Q, E>(
    at: &Option<PlaceAt<P>>,
    f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
) -> Result<Option<PlaceAt<Q>>, E> {
    match at {
        None => Ok(None),
        Some(at) => at.map_params("at.", f).map(Some),
    }
}

impl PlaceAt<f64> {
    /// Checks the values.
    pub fn check_values(&self) -> Result<(), String> {
        for (value, what) in [
            (self.offset, "offset"),
            (self.offset1, "first offset"),
            (self.offset2, "second offset"),
            (self.angle, "angle"),
        ] {
            if value.is_some_and(|v| !v.is_finite()) {
                return Err(format!("the placement's {what} must be a number"));
            }
        }
        Ok(())
    }
}

/// A resolved placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    /// The part's frame: its origin, its axis as the normal, its x axis.
    pub frame: SketchFrame,
    /// The `axis` form: the seat (its diameter NaN on an axis of its own).
    pub seat: Option<SeatFrame>,
}

impl Placed {
    /// The seat of a shaft or a hole with a diameter.
    pub fn round(&self) -> Option<&SeatFrame> {
        self.seat.as_ref().filter(|seat| seat.diameter.is_finite())
    }

    /// As the `place_frame` query gives it.
    pub fn to_json(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "origin": self.frame.origin,
            "x_axis": self.frame.x_axis,
            "y_axis": self.frame.y_axis,
            "axis": self.frame.normal(),
        });
        if let Some(seat) = &self.seat {
            value["seat"] = seat.to_json();
        }
        value
    }
}

/// Resolves a placement where `r` gives the geometry.
pub(crate) fn resolve<R: Resolver>(r: &mut R, at: &PlaceAt<f64>) -> Result<Placed, String> {
    let (frame, seat) = match at.form()? {
        Form::Axis(axis) => {
            let seat = Seat {
                shaft: axis.clone(),
                link: None,
                offset: at.offset,
                flip: at.flip,
                near: at.near,
            };
            let round = round_of(r, axis).map_err(|e| format!("axis: {e}"))?;
            let seat = seat.frame(&round);
            (seat.sketch_frame(), Some(seat))
        }
        Form::Point(point) => (lifted(point_frame(r, point, at.plane.as_ref())?, at), None),
        Form::Face(face) => (lifted(face_frame(r, at, face)?, at), None),
    };
    let frame = aligned(r, frame, at.align.as_ref(), at.align_side, at.angle)?;
    Ok(Placed { frame, seat })
}

/// `flip` and `offset` of a point's or a face's frame.
fn lifted(frame: SketchFrame, at: &PlaceAt<f64>) -> SketchFrame {
    let frame = if at.flip {
        SketchFrame {
            y_axis: scale(frame.y_axis, -1.0),
            ..frame
        }
    } else {
        frame
    };
    SketchFrame {
        origin: add(
            frame.origin,
            scale(frame.normal(), at.offset.unwrap_or(0.0)),
        ),
        ..frame
    }
}

/// The frame at a point: its sketch's (or `plane`'s) frame moved to it.
fn point_frame<R: Resolver>(
    r: &mut R,
    point: &GeomRef,
    plane: Option<&GeomRef>,
) -> Result<SketchFrame, String> {
    let (at, own): (Vec3, Option<SketchFrame>) = match point {
        GeomRef::SketchPoint { sketch, .. } => {
            let at = resolve_point(r, point).map_err(|e| format!("point: {e}"))?;
            (at, Some(r.sketch(*sketch)?.frame))
        }
        GeomRef::SketchCurves { sketch, curves } if curves.len() == 1 => {
            let output = r.sketch(*sketch)?;
            let center = match output.solved.curves.get(&curves[0]) {
                Some(Curve2::Circle { center, .. } | Curve2::Arc { center, .. }) => *center,
                _ => {
                    return Err(format!(
                        "point: {} has no circle or arc {}",
                        r.name(*sketch),
                        curves[0].curve_name()
                    ));
                }
            };
            (output.frame.point(center), Some(output.frame))
        }
        GeomRef::SketchCurves { .. } => {
            return Err("point: pick one circle or arc of the sketch".to_owned());
        }
        GeomRef::Edge { .. } => {
            let at = resolve_point(r, point).map_err(|e| format!("point: {e}"))?;
            let axis = resolve_axis(r, point).map_err(|e| format!("point: {e}"))?;
            (
                at,
                Some(DatumPlane::from_normal(at, axis.direction, None).frame()),
            )
        }
        _ => (
            resolve_point(r, point).map_err(|e| format!("point: {e}"))?,
            None,
        ),
    };
    let frame = match plane {
        Some(plane) => resolve_plane(r, plane)
            .map_err(|e| format!("plane: {e}"))?
            .frame(),
        None => own.ok_or(
            "a vertex, a construction point or the origin needs a plane or a planar face for its axis",
        )?,
    };
    Ok(SketchFrame {
        origin: at,
        ..frame
    })
}

/// The frame on a face: its plane's frame moved to the point at the
/// offsets from the two lines (or from an origin plane's axes).
fn face_frame<R: Resolver>(
    r: &mut R,
    at: &PlaceAt<f64>,
    face: &GeomRef,
) -> Result<SketchFrame, String> {
    let (plane, middle) = match face {
        GeomRef::Face { body, face: name } => {
            let shape = r.shape(*body)?;
            let found = super::renamed::face_for(r, &shape, name)?;
            let plane = r.kernel().face_plane(&shape, &found).map_err(|e| {
                crate::notice::placed(
                    "the face is not planar",
                    crate::notice::NoticePlace::face(name.to_string()).in_body(&body.to_string()),
                    format!("face {name} is not planar ({e})"),
                )
            })?;
            let normal = unit(plane.normal).ok_or_else(|| {
                crate::notice::placed(
                    "the face has no normal",
                    crate::notice::NoticePlace::face(name.to_string()).in_body(&body.to_string()),
                    format!("face {name} has no normal"),
                )
            })?;
            (
                DatumPlane::on_plane(plane.origin, normal),
                Some(plane.origin),
            )
        }
        other => (
            resolve_plane(r, other).map_err(|e| format!("face: {e}"))?,
            None,
        ),
    };
    let frame = plane.frame();
    let offsets = [at.offset1.unwrap_or(0.0), at.offset2.unwrap_or(0.0)];
    let origin = match (&at.edge1, &at.edge2) {
        (Some(a), Some(b)) => {
            let lines = [
                resolve_axis(r, a).map_err(|e| format!("edge1: {e}"))?,
                resolve_axis(r, b).map_err(|e| format!("edge2: {e}"))?,
            ];
            let side = middle.or(at.near).ok_or(
                "a point on a plane measured from two lines needs near, the picked point, for their sides",
            )?;
            offset_point(&frame, &lines, offsets, side)?
        }
        (None, None) if matches!(face, GeomRef::Origin(_)) => frame.point(offsets),
        _ => {
            return Err(
                "a point on a face is measured from two of its edges (only an origin plane measures from its origin axes)"
                    .to_owned(),
            );
        }
    };
    Ok(SketchFrame { origin, ..frame })
}

/// The point of the frame's plane at `offsets` from the lines, positive on
/// the side of each line `side` is on.
pub(crate) fn offset_point(
    frame: &SketchFrame,
    lines: &[DatumAxis; 2],
    offsets: [f64; 2],
    side: Vec3,
) -> Result<Vec3, String> {
    let n = frame.normal();
    let mut rows = [[0.0; 2]; 2];
    let mut right = [0.0; 2];
    for (k, line) in lines.iter().enumerate() {
        let across = cross(n, line.direction);
        let length = norm(across);
        if length < 1.0e-9 {
            return Err(
                "a reference edge of the placement runs along the face's normal".to_owned(),
            );
        }
        let mut across = scale(across, 1.0 / length);
        if dot(sub(side, line.origin), across) < 0.0 {
            across = scale(across, -1.0);
        }
        rows[k] = [dot(frame.x_axis, across), dot(frame.y_axis, across)];
        right[k] = offsets[k] - dot(sub(frame.origin, line.origin), across);
    }
    let det = rows[0][0] * rows[1][1] - rows[0][1] * rows[1][0];
    if det.abs() < 1.0e-9 {
        return Err("the reference edges of the placement are parallel".to_owned());
    }
    let u = (right[0] * rows[1][1] - rows[0][1] * right[1]) / det;
    let v = (rows[0][0] * right[1] - right[0] * rows[1][0]) / det;
    Ok(frame.point([u, v]))
}

/// `align` and `angle`: the x axis along a line, then turned.
fn aligned<R: Resolver>(
    r: &mut R,
    frame: SketchFrame,
    align: Option<&GeomRef>,
    side: Option<Vec3>,
    angle: Option<f64>,
) -> Result<SketchFrame, String> {
    let n = frame.normal();
    let mut frame = frame;
    if let Some(reference) = align {
        // A line's direction, or a planar face's (a plane's) normal
        // (mitcad#656: a shaft's flat).
        let (d, face) = match resolve_axis(r, reference) {
            Ok(axis) => (axis.direction, false),
            Err(error) => match resolve_plane(r, reference) {
                Ok(plane) => (plane.normal(), true),
                Err(_) => return Err(format!("align: {error}")),
            },
        };
        let along = sub(d, scale(n, dot(d, n)));
        let length = norm(along);
        if length < 1.0e-9 {
            return Err(if face {
                "the face to align to lies square to the placement's axis: pick a face along it, such as a shaft's flat".to_owned()
            } else {
                "the edge to align to runs along the placement's axis".to_owned()
            });
        }
        let mut x = scale(along, 1.0 / length);
        // The y axis toward `side` (align_side): the line reversed if need be.
        if side.is_some_and(|toward| dot(cross(n, x), toward) < 0.0) {
            x = scale(x, -1.0);
        }
        frame.x_axis = x;
        frame.y_axis = cross(n, x);
    }
    Ok(turned(frame, angle.unwrap_or(0.0)))
}

/// A frame turned about its normal by `angle`.
pub(crate) fn turned(frame: SketchFrame, angle: f64) -> SketchFrame {
    if angle == 0.0 {
        return frame;
    }
    let (s, c) = angle.sin_cos();
    SketchFrame {
        origin: frame.origin,
        x_axis: add(scale(frame.x_axis, c), scale(frame.y_axis, s)),
        y_axis: sub(scale(frame.y_axis, c), scale(frame.x_axis, s)),
    }
}

impl<K: Kernel> EvalContext<'_, K> {
    /// A placement at this point of the timeline (what it reads recorded
    /// for the recompute cache).
    pub fn place(&mut self, at: &PlaceAt<f64>) -> Result<Placed, String> {
        resolve(self, at).map_err(|e| format!("at: {e}"))
    }

    /// The same with the placement's parameters read here.
    pub fn place_def(&mut self, at: &PlaceAt) -> Result<Placed, String> {
        let at = at.map_params("at.", &mut |_, id: &ParamId| self.param(*id))?;
        at.check_values()?;
        self.place(&at)
    }
}

impl<K: Kernel> Document<K> {
    /// A placement at the timeline marker: what a panel shows before the
    /// feature is made, and the axes of the Gear Pair and Belt Drive
    /// queries.
    pub fn place_frame(&self, at: &PlaceAt<f64>) -> Result<Placed, String> {
        let mut resolver = self.resolver();
        resolve(&mut resolver, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Vec3, b: Vec3) -> bool {
        norm(sub(a, b)) < 1.0e-9
    }

    #[test]
    fn offsets_from_two_lines_place_the_point_on_the_side_given() {
        let frame = SketchFrame::XY;
        // Lines along x through y = 10 and along y through x = 20.
        let lines = [
            DatumAxis {
                origin: [0.0, 10.0, 0.0],
                direction: [1.0, 0.0, 0.0],
            },
            DatumAxis {
                origin: [20.0, 0.0, 0.0],
                direction: [0.0, 1.0, 0.0],
            },
        ];
        // The face's middle above and left of both.
        let p = offset_point(&frame, &lines, [5.0, 3.0], [0.0, 30.0, 0.0]).unwrap();
        assert!(close(p, [17.0, 15.0, 0.0]), "{p:?}");
        // From the other side.
        let p = offset_point(&frame, &lines, [5.0, 3.0], [40.0, 0.0, 0.0]).unwrap();
        assert!(close(p, [23.0, 5.0, 0.0]), "{p:?}");
        // A line out of the plane counts by its projection.
        let raised = [
            DatumAxis {
                origin: [0.0, 10.0, 7.0],
                ..lines[0]
            },
            lines[1],
        ];
        let p = offset_point(&frame, &raised, [5.0, 3.0], [0.0, 30.0, 0.0]).unwrap();
        assert!(close(p, [17.0, 15.0, 0.0]), "{p:?}");
        // Parallel lines and a line along the normal are refused.
        let parallel = [
            lines[0],
            DatumAxis {
                origin: [0.0, 0.0, 0.0],
                direction: [1.0, 0.0, 0.0],
            },
        ];
        assert!(offset_point(&frame, &parallel, [1.0, 1.0], [0.0, 30.0, 0.0]).is_err());
        let upright = [
            lines[0],
            DatumAxis {
                origin: [0.0, 0.0, 0.0],
                direction: [0.0, 0.0, 1.0],
            },
        ];
        assert!(offset_point(&frame, &upright, [1.0, 1.0], [0.0, 30.0, 0.0]).is_err());
    }

    #[test]
    fn flip_offset_and_angle_move_and_turn_the_frame() {
        let at = |offset: f64, flip: bool| PlaceAt::<f64> {
            axis: None,
            point: Some(GeomRef::FixedPoint([0.0; 3])),
            plane: None,
            face: None,
            edge1: None,
            offset1: None,
            edge2: None,
            offset2: None,
            near: None,
            offset: Some(offset),
            flip,
            angle: None,
            align: None,
            align_side: None,
        };
        let frame = SketchFrame {
            origin: [1.0, 2.0, 3.0],
            ..SketchFrame::XY
        };
        let up = lifted(frame, &at(5.0, false));
        assert!(close(up.origin, [1.0, 2.0, 8.0]));
        let down = lifted(frame, &at(5.0, true));
        assert!(close(down.origin, [1.0, 2.0, -2.0]));
        assert!(close(down.normal(), [0.0, 0.0, -1.0]));
        assert!(close(down.x_axis, [1.0, 0.0, 0.0]));
        let quarter = turned(frame, std::f64::consts::FRAC_PI_2);
        assert!(close(quarter.x_axis, [0.0, 1.0, 0.0]), "{quarter:?}");
        assert!(close(quarter.y_axis, [-1.0, 0.0, 0.0]), "{quarter:?}");
        assert!(close(quarter.normal(), [0.0, 0.0, 1.0]));
    }

    #[test]
    fn a_placement_is_one_form_and_round_trips() {
        let value = serde_json::json!({"face": "xy", "offset1": 20, "offset2": 15, "flip": true});
        let at: PlaceAt<f64> = serde_json::from_value(value.clone()).unwrap();
        assert!(matches!(at.form(), Ok(Form::Face(_))));
        assert_eq!(
            serde_json::to_value(&at).unwrap(),
            serde_json::json!({"face": "xy", "offset1": 20.0, "offset2": 15.0, "flip": true})
        );
        let both: PlaceAt<f64> =
            serde_json::from_value(serde_json::json!({"axis": "z", "point": "origin"})).unwrap();
        assert!(both.form().is_err());
        let unknown =
            serde_json::from_value::<PlaceAt<f64>>(serde_json::json!({"axis": "z", "centre": 1}));
        assert!(unknown.is_err());
    }
}
