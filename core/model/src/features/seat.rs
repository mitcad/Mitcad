// SPDX-License-Identifier: MIT
//! Seats on shafts (mitcad#578): a part that sits on a round shaft or in a
//! round hole (a pulley, a gear, a printable bearing) is placed by it
//! instead of by a plane and a centre. The shaft is picked as a circular
//! edge (a shaft's end edge, a hole's rim) or as its cylindrical face (a
//! shaft's side, a hole's wall), on a body of the feature's own component
//! or of another one (`link`, as a combine's tools in occurrences:
//! `links.rs`). The seat resolves into
//!
//! - the end the part is measured from: the picked edge's end; on a face
//!   the end nearer `near` (where the face was picked, in the shaft's
//!   component's coordinates), the face's low end
//!   along its axis without it; `flip` takes the other end;
//! - `axis`: along the shaft's axis from that end into the shaft (into the
//!   hole);
//! - `origin`: `offset` along the axis from that end;
//! - `diameter`: the shaft's (the hole's), which a bore follows.
//!
//! The shaft may also be an axis of its own (mitcad#656): an origin or
//! construction axis, a sketch line, circle or arc, a fixed axis. It is
//! measured from its origin point along its direction and has no length
//! and no diameter (NaN): a seated part keeps its own bore.
//!
//! A part seated so lies on the plane through `origin` normal to `axis`
//! and runs along `axis` from it: a pulley at offset 0 from a shaft's end
//! starts at the end and runs along the shaft; flipped, it starts at the
//! other end and runs back. The one resolver (`round_of`) and the frame
//! ([`Seat::frame`]) serve every seated feature; [`EvalContext::seat`]
//! resolves a seat during evaluation (recording what it reads),
//! [`Document::seat_frame`] at the timeline marker (the panels' queries).

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Resolver, Want};
use super::{CheckContext, EvalContext, FeatureDef, References, is_false};
use crate::datum::{DatumPlane, Vec3};
use crate::document::Document;
use crate::ids::{BodyUid, ComponentUid};
use crate::kernel::Kernel;
use crate::links::{OccurrenceLink, OccurrenceLinkExt};
use crate::parameters::ParamId;
use crate::profile::SketchFrame;
use crate::transform::Transform;
use mitcad_vec3::dot;
use mitcad_vec3::{add, norm, scale, sub, unit};

fn none<P>() -> Option<P> {
    None
}

/// Where a part sits on a shaft or in a hole (see the module
/// documentation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seat<P = ParamId> {
    /// A circular edge or a cylindrical face of a body.
    pub shaft: GeomRef,
    /// The shaft's body is another component's, seen where `link` says
    /// (`{"source": "O1", "target": "O2"}`, an [`OccurrenceLink`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<OccurrenceLink>,
    /// Along the axis from the end, mm; 0 without it.
    #[serde(default = "none", skip_serializing_if = "Option::is_none")]
    pub offset: Option<P>,
    /// Measured from the other end, the part turned to face back along
    /// the shaft.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
    /// Where a face was picked, mm, in the coordinates of the shaft's
    /// component (as a pick gives it): the end nearer it is the end
    /// measured from, whatever moves the shaft's occurrence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub near: Option<Vec3>,
}

impl<P> Seat<P> {
    /// Converts the value fields; the offset's slot is `<prefix>offset`
    /// (`seat.offset`).
    // Written by hand: the slot takes the owner's prefix.
    pub fn map_params<Q, E>(
        &self,
        prefix: &str,
        f: &mut dyn FnMut(&str, &P) -> Result<Q, E>,
    ) -> Result<Seat<Q>, E> {
        Ok(Seat {
            shaft: self.shaft.clone(),
            link: self.link.clone(),
            offset: self
                .offset
                .as_ref()
                .map(|v| f(&format!("{prefix}offset"), v))
                .transpose()?,
            flip: self.flip,
            near: self.near,
        })
    }

    /// The shaft's body.
    pub fn body(&self) -> Option<BodyUid> {
        match &self.shaft {
            GeomRef::Edge { body, .. } | GeomRef::Face { body, .. } => Some(*body),
            _ => None,
        }
    }

    /// Adds what the shaft refers to.
    pub fn add_to(&self, references: &mut References) {
        self.shaft.add_to(references);
    }

    /// Checks the shaft before the feature: an edge or a face of a body
    /// before it (whether it is round only evaluation can tell).
    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        // A body's edge or face, or an axis of its own (mitcad#656).
        let on_body = self.body().is_some();
        self.shaft
            .check(
                ctx,
                if on_body {
                    Want::EdgeOrFace
                } else {
                    Want::Axis
                },
            )
            .map_err(|e| format!("shaft: {e}"))?;
        if !on_body && self.link.is_some() {
            return Err(
                "an axis of its own takes no link: only a body's shaft is another component's"
                    .to_owned(),
            );
        }
        if self.near.is_some_and(|p| p.iter().any(|v| !v.is_finite())) {
            return Err("the seat's picked point must be finite".to_owned());
        }
        Ok(())
    }
}

impl Seat<f64> {
    /// Checks the values.
    pub fn check_values(&self) -> Result<(), String> {
        match self.offset {
            Some(v) if !v.is_finite() => Err("the seat's offset must be a number".to_owned()),
            _ => Ok(()),
        }
    }

    /// The seat on `round`, a shaft resolved in the feature's coordinates
    /// (its end chosen in its own: [`Round::ended`]).
    pub fn frame(&self, round: &Round) -> SeatFrame {
        let high = round.at(round.length);
        let at_high = round.ended(self.near).picked_high == Some(true);
        let at_high = at_high != self.flip;
        let (end, axis) = if at_high {
            (high, scale(round.axis, -1.0))
        } else {
            (round.low, round.axis)
        };
        SeatFrame {
            origin: add(end, scale(axis, self.offset.unwrap_or(0.0))),
            axis,
            end,
            diameter: round.diameter,
            length: round.length,
            hole: round.hole,
        }
    }
}

/// A round shaft or hole: its cylindrical face's axis from its low end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Round {
    /// On the axis at the face's low end.
    pub low: Vec3,
    /// The unit axis, from the low end to the high one.
    pub axis: Vec3,
    /// The face's extent along the axis.
    pub length: f64,
    pub diameter: f64,
    /// A hole's wall (the material outside).
    pub hole: bool,
    /// The end measured from, once known (an edge's own): whether the
    /// high end.
    pub picked_high: Option<bool>,
}

impl Round {
    /// The axis point `along` from the low end.
    fn at(&self, along: f64) -> Vec3 {
        add(self.low, scale(self.axis, along))
    }

    /// With the end measured from: an edge's own, else the end nearer
    /// `near` (in the round's coordinates), the low end without it.
    pub fn ended(&self, near: Option<Vec3>) -> Round {
        let high = self.picked_high.unwrap_or_else(|| {
            near.is_some_and(|p| dot(sub(p, self.low), self.axis) > self.length / 2.0)
        });
        Round {
            picked_high: Some(high),
            ..*self
        }
    }

    /// Moved by a rigid transform (from another component's coordinates).
    pub fn transformed(&self, t: &Transform) -> Round {
        Round {
            low: t.apply_point(self.low),
            axis: unit(t.apply_vector(self.axis)).unwrap_or(self.axis),
            ..*self
        }
    }
}

/// A resolved seat (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeatFrame {
    /// On the axis, `offset` from `end`.
    pub origin: Vec3,
    /// The unit axis from `end` into the shaft (into the hole).
    pub axis: Vec3,
    /// The end the offset is measured from, on the axis.
    pub end: Vec3,
    pub diameter: f64,
    /// The shaft's cylindrical face's length along the axis.
    pub length: f64,
    pub hole: bool,
}

impl SeatFrame {
    /// The frame of the seated part's plane: the origin on the axis, the
    /// normal along it, x by the rule of planes without one
    /// ([`DatumPlane::from_normal`]).
    pub fn sketch_frame(&self) -> SketchFrame {
        DatumPlane::from_normal(self.origin, self.axis, None).frame()
    }

    /// As the `seat_frame` query and the panels show it.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "origin": self.origin,
            "axis": self.axis,
            "end": self.end,
            "diameter": self.diameter,
            "length": self.length,
            "hole": self.hole,
        })
    }
}

const NOT_ROUND: &str = "pick a shaft's circular end edge or its cylindrical side face \
                         (a hole's rim or wall)";

/// The round shaft or hole a circular edge or a cylindrical face is of, in
/// the coordinates `r` gives the body in. An edge must end a cylindrical
/// face of its own radius (a shaft's end edge, a hole's rim).
pub(crate) fn round_of<R: Resolver>(r: &mut R, shaft: &GeomRef) -> Result<Round, String> {
    match shaft {
        GeomRef::Face { body, face } => {
            let shape = r.shape(*body)?;
            let found = super::renamed::face_for(r, &shape, face)?;
            if matches!(r.kernel().count_faces(&shape, &found), Ok(0)) {
                return Err(super::geom_ref::placed_at(
                    crate::notice::NoticePlace::face(face.to_string()),
                    *body,
                    "the body has no such face",
                    format!("body {body} has no face {face}"),
                ));
            }
            let cylinder = r.kernel().face_cylinder(&shape, &found).map_err(|e| {
                super::geom_ref::placed_at(
                    crate::notice::NoticePlace::face(face.to_string()),
                    *body,
                    &format!("the face is not round: {NOT_ROUND}"),
                    format!("face {face} is not round ({e}): {NOT_ROUND}"),
                )
            })?;
            let axis = unit(cylinder.axis.direction).ok_or_else(|| {
                super::geom_ref::placed_at(
                    crate::notice::NoticePlace::face(face.to_string()),
                    *body,
                    &format!("the face has no axis: {NOT_ROUND}"),
                    format!("face {face} has no axis: {NOT_ROUND}"),
                )
            })?;
            Ok(Round {
                low: cylinder.axis.origin,
                axis,
                length: cylinder.length,
                diameter: 2.0 * cylinder.radius,
                hole: cylinder.internal,
                picked_high: None,
            })
        }
        GeomRef::Edge { body, edge } => {
            let shape = r.shape(*body)?;
            let found = super::renamed::edge_for(r, &shape, edge)?;
            let curve = r.kernel().edge_geometry(&shape, &found).map_err(|e| {
                crate::notice::about_place(
                    crate::notice::NoticePlace::edge(edge.to_string()),
                    &e.to_string(),
                )
            })?;
            let (center, normal, radius) = match curve {
                crate::datum::CurveGeometry::Circle {
                    center,
                    normal,
                    radius,
                    ..
                } => (center, normal, radius),
                other => {
                    return Err(super::geom_ref::placed_at(
                        crate::notice::NoticePlace::edge(edge.to_string()),
                        *body,
                        &format!("the edge is not round: {NOT_ROUND}"),
                        format!("edge {edge} is not round (a {}): {NOT_ROUND}", other.kind()),
                    ));
                }
            };
            let normal = unit(normal).ok_or_else(|| {
                super::geom_ref::placed_at(
                    crate::notice::NoticePlace::edge(edge.to_string()),
                    *body,
                    "the edge has no axis",
                    format!("edge {edge} has no axis"),
                )
            })?;
            let tolerance = 1.0e-6 * radius.max(1.0);
            for face in found.faces() {
                let face = super::renamed::face_for(r, &shape, face)?;
                let Ok(cylinder) = r.kernel().face_cylinder(&shape, &face) else {
                    continue;
                };
                let Some(axis) = unit(cylinder.axis.direction) else {
                    continue;
                };
                let from = sub(center, cylinder.axis.origin);
                let along = dot(from, axis);
                let off_axis = norm(sub(from, scale(axis, along)));
                if dot(axis, normal).abs() < 1.0 - 1.0e-6
                    || off_axis > tolerance
                    || (cylinder.radius - radius).abs() > tolerance
                {
                    continue;
                }
                return Ok(Round {
                    low: cylinder.axis.origin,
                    axis,
                    length: cylinder.length,
                    diameter: 2.0 * cylinder.radius,
                    hole: cylinder.internal,
                    picked_high: Some(along > cylinder.length / 2.0),
                });
            }
            Err(format!(
                "edge {edge} does not end a cylindrical face: {NOT_ROUND}"
            ))
        }
        // An axis of its own (mitcad#656): from its origin point along it,
        // without a length or a diameter.
        GeomRef::Origin(_)
        | GeomRef::Datum(_)
        | GeomRef::SketchCurves { .. }
        | GeomRef::FixedAxis { .. }
        | GeomRef::Mount { .. } => {
            let line = super::geom_ref::resolve_axis(r, shaft)
                .map_err(|e| format!("{e}: {NOT_ROUND}, or an axis"))?;
            Ok(Round {
                low: line.origin,
                axis: unit(line.direction).ok_or("the axis has no direction")?,
                length: 0.0,
                diameter: f64::NAN,
                hole: false,
                picked_high: Some(false),
            })
        }
        other => Err(format!("{other} is not a shaft: {NOT_ROUND}")),
    }
}

/// A resolver that gives one body as it is (another component's body in
/// its own coordinates) and everything else as `inner` does.
struct OneBody<'a, R: Resolver> {
    inner: &'a mut R,
    body: BodyUid,
    shape: <R::Kernel as Kernel>::Shape,
}

impl<R: Resolver> Resolver for OneBody<'_, R> {
    type Kernel = R::Kernel;

    fn kernel(&self) -> &R::Kernel {
        self.inner.kernel()
    }

    fn name(&self, uid: crate::ids::FeatureUid) -> String {
        self.inner.name(uid)
    }

    fn datum(&mut self, uid: crate::ids::FeatureUid) -> Result<crate::datum::Datum, String> {
        self.inner.datum(uid)
    }

    fn shape(&mut self, body: BodyUid) -> Result<<R::Kernel as Kernel>::Shape, String> {
        if body == self.body {
            Ok(self.shape.clone())
        } else {
            self.inner.shape(body)
        }
    }

    fn sketch(
        &mut self,
        uid: crate::ids::FeatureUid,
    ) -> Result<std::sync::Arc<super::SketchOutput>, String> {
        self.inner.sketch(uid)
    }

    fn renamed(&mut self, what: &str, stored: &str, found: &str, maker: crate::ids::FeatureUid) {
        self.inner.renamed(what, stored, found, maker);
    }

    fn feature_curve(
        &mut self,
        feature: crate::ids::FeatureUid,
    ) -> Result<std::sync::Arc<Vec<crate::sweeps::PathCurve>>, String> {
        self.inner.feature_curve(feature)
    }

    fn mount(&mut self, feature: crate::ids::FeatureUid, name: &str) -> Result<Transform, String> {
        self.inner.mount(feature, name)
    }
}

impl<K: Kernel> EvalContext<'_, K> {
    /// A seat at this point of the timeline, in the feature's coordinates:
    /// a linked shaft is read in its component and moved by the
    /// placements here (recorded for the recompute cache).
    pub fn seat(&mut self, seat: &Seat<f64>) -> Result<SeatFrame, String> {
        let round = match (&seat.link, seat.body()) {
            (Some(link), Some(body)) => {
                let (component, transform) = self
                    .linked(link)
                    .map_err(|e| format!("shaft: body {body}: {e}"))?;
                let shape = self.body_in(component, body)?;
                let mut one = OneBody {
                    inner: self,
                    body,
                    shape,
                };
                round_of(&mut one, &seat.shaft)?
                    .ended(seat.near)
                    .transformed(&transform)
            }
            _ => round_of(self, &seat.shaft)?,
        };
        Ok(seat.frame(&round))
    }
}

impl<K: Kernel> Document<K> {
    /// A seat at the timeline marker, in the coordinates of `component`
    /// (the seated feature's; a linked shaft moved by the placements at
    /// the marker): what a panel shows before the feature is made.
    pub fn seat_frame(
        &self,
        seat: &Seat<f64>,
        component: ComponentUid,
    ) -> Result<SeatFrame, String> {
        let mut resolver = self.resolver();
        let round = round_of(&mut resolver, &seat.shaft)?.ended(seat.near);
        let round = match &seat.link {
            Some(link) => {
                let transform = link.transform(self.assembly(), component, &mut |o| {
                    self.placement(o).unwrap_or(Transform::IDENTITY)
                })?;
                round.transformed(&transform)
            }
            None => round,
        };
        Ok(seat.frame(&round))
    }
}

/// The seat of a pulley, a gear or a printable bearing.
fn seat_of<P>(def: &FeatureDef<P>) -> Option<&Seat<P>> {
    match def {
        FeatureDef::Pulley(p) => p.seat.as_ref(),
        FeatureDef::Gear(g) => g.seat.as_ref(),
        FeatureDef::PrintableBearing(b) => b.seat.as_ref(),
        _ => None,
    }
}

impl<P> FeatureDef<P> {
    /// A seat's link to another component's shaft, with the shaft's body
    /// (copies, moves and reparents carry it as the other features'
    /// links).
    pub(crate) fn seat_link(&self) -> Option<(BodyUid, &OccurrenceLink)> {
        let seat = seat_of(self)?;
        Some((seat.body()?, seat.link.as_ref()?))
    }

    /// The same, to change.
    pub(crate) fn seat_link_mut(&mut self) -> Option<(BodyUid, &mut OccurrenceLink)> {
        let seat = match self {
            FeatureDef::Pulley(p) => p.seat.as_mut(),
            FeatureDef::Gear(g) => g.seat.as_mut(),
            FeatureDef::PrintableBearing(b) => b.seat.as_mut(),
            _ => None,
        }?;
        let body = seat.body()?;
        Some((body, seat.link.as_mut()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round() -> Round {
        // A shaft along z from z = 10 to 40, 8 across.
        Round {
            low: [5.0, 0.0, 10.0],
            axis: [0.0, 0.0, 1.0],
            length: 30.0,
            diameter: 8.0,
            hole: false,
            picked_high: None,
        }
    }

    fn seat(offset: f64, flip: bool, near: Option<Vec3>) -> Seat<f64> {
        Seat {
            shaft: GeomRef::FixedPoint([0.0; 3]),
            link: None,
            offset: Some(offset),
            flip,
            near,
        }
    }

    fn close(a: Vec3, b: Vec3) -> bool {
        norm(sub(a, b)) < 1.0e-9
    }

    #[test]
    fn a_face_seat_measures_from_the_end_nearer_its_pick() {
        // Without a pick: the low end, along the axis.
        let frame = seat(5.0, false, None).frame(&round());
        assert!(close(frame.origin, [5.0, 0.0, 15.0]), "{frame:?}");
        assert!(close(frame.axis, [0.0, 0.0, 1.0]));
        assert!(close(frame.end, [5.0, 0.0, 10.0]));
        // Picked near the top: from the top, down the shaft.
        let frame = seat(5.0, false, Some([9.0, 0.0, 36.0])).frame(&round());
        assert!(close(frame.origin, [5.0, 0.0, 35.0]), "{frame:?}");
        assert!(close(frame.axis, [0.0, 0.0, -1.0]));
        // Flipped: from the other end.
        let frame = seat(5.0, true, Some([9.0, 0.0, 36.0])).frame(&round());
        assert!(close(frame.origin, [5.0, 0.0, 15.0]), "{frame:?}");
        assert!(close(frame.axis, [0.0, 0.0, 1.0]));
        assert_eq!(frame.diameter, 8.0);
    }

    #[test]
    fn an_edge_seat_measures_from_its_end_and_moves_with_its_component() {
        let picked = Round {
            picked_high: Some(true),
            ..round()
        };
        // A pick near the other end does not matter for an edge.
        let frame = seat(2.0, false, Some([0.0, 0.0, 0.0])).frame(&picked);
        assert!(close(frame.origin, [5.0, 0.0, 38.0]), "{frame:?}");
        let frame = seat(2.0, true, None).frame(&picked);
        assert!(close(frame.origin, [5.0, 0.0, 12.0]), "{frame:?}");
        // Moved 100 along x: the axis stays, the ends move.
        let moved = picked.transformed(&Transform::translation([100.0, 0.0, 0.0]));
        let frame = seat(2.0, false, None).frame(&moved);
        assert!(close(frame.origin, [105.0, 0.0, 38.0]), "{frame:?}");
    }
}
