// SPDX-License-Identifier: MIT
//! What profile features refer to besides their profiles: the objects a
//! sweep runs up to or starts from. Planes, revolution axes (also sketch
//! lines) and other geometry are references ([`GeomRef`]); faces and bodies
//! as sweep targets, with their options, are added here.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::geom_ref::{GeomRef, Want};
use super::{CheckContext, EvalContext, References, is_false};
use crate::datum::CurveGeometry;
use crate::ids::BodyUid;
use crate::kernel::{Axis, Kernel, Plane, Target};
use crate::links::OccurrenceLink;
use crate::profile::SketchFrame;
use crate::topo::FaceName;
use mitcad_vec3::dot;

/// An object a sweep runs up to (the "To Object" extent) or starts
/// from ("From Object", planes and planar faces only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtentObject {
    /// An unbounded plane: an origin plane (`"xy"`), a construction plane
    /// (`"F5"`), a planar face (`{"body", "face"}`) continued past its
    /// edges, or a fixed plane.
    Plane { plane: GeomRef },
    /// A face of a body. Its surface continues past its edges, unless
    /// `chained`: then the face and the faces next to it (`isChained` in
    /// .f3d designs).
    Face {
        body: BodyUid,
        face: FaceName,
        #[serde(default, skip_serializing_if = "is_false")]
        chained: bool,
    },
    /// A body: up to its first face reached, or `through` it to its far
    /// side (`isMinimumSolution = false` in .f3d designs).
    Body {
        body: BodyUid,
        #[serde(default, skip_serializing_if = "is_false")]
        through: bool,
    },
    /// A point (mitcad#1329): a sketch point (`{"sketch", "point"}`), a
    /// vertex (`{"body", "vertex"}`), a construction point (`"F5"`) or
    /// another point reference. It stands for the plane through the point
    /// parallel to the profile's plane (an extrusion's) or at right angles
    /// to the axis (a hole's). Revolutions do not take one.
    Point { point: GeomRef },
    /// A straight line (mitcad#1329): a sketch line (`{"sketch",
    /// "curve"}`), a straight edge (`{"body", "edge"}`), a construction or
    /// origin axis or a fixed axis. It must lie in a plane parallel to the
    /// profile's plane (an extrusion's; for a hole, at right angles to its
    /// axis), within [`LINE_ANGLE`], and stands for that plane.
    /// Revolutions do not take one.
    Line { line: GeomRef },
}

/// How far a line object may lean out of the plane it stands for
/// (mitcad#1329), in radians.
pub const LINE_ANGLE: f64 = 1e-6;

/// What a point or a line object stands for (mitcad#1329): the plane
/// through it with this normal. `lies` says where a line must lie, for the
/// message when it does not ("in a plane parallel to the profile's
/// plane").
#[derive(Debug, Clone, Copy)]
pub struct Across<'a> {
    pub normal: [f64; 3],
    pub lies: &'a str,
}

impl ExtentObject {
    pub fn add_references(&self, references: &mut References) {
        match self {
            Self::Plane { plane } => plane.add_to(references),
            Self::Face { body, face, .. } => {
                references.body(*body);
                references.features.extend(face.features());
            }
            Self::Body { body, .. } => references.body(*body),
            Self::Point { point } => point.add_to(references),
            Self::Line { line } => line.add_to(references),
        }
    }

    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        match self {
            Self::Plane { plane } => plane.check(ctx, Want::Plane),
            Self::Point { point } => point.check(ctx, Want::Point),
            // A straight line; whether it lies in a plane parallel to the
            // profile only evaluation can tell.
            Self::Line { line } => line.check(ctx, Want::Line),
            Self::Face { body, face, .. } => {
                ctx.body(*body)?;
                for feature in face.features() {
                    ctx.feature(feature).map_err(|e| {
                        crate::notice::about_place(
                            crate::notice::NoticePlace::face(face.to_string()),
                            &e.to_string(),
                        )
                    })?;
                }
                Ok(())
            }
            Self::Body { body, .. } => ctx.body(*body),
        }
    }

    /// True for the objects a sweep may start from (a point as the plane
    /// through it).
    pub fn is_planar(&self) -> bool {
        !matches!(self, Self::Body { .. })
    }

    /// True for a point or a line (mitcad#1329), which only extrusions and
    /// holes take.
    pub fn is_point_or_line(&self) -> bool {
        matches!(self, Self::Point { .. } | Self::Line { .. })
    }

    /// The bodies of the object (the face's or the body itself).
    pub fn body(&self) -> Option<BodyUid> {
        match self {
            Self::Plane { .. } | Self::Point { .. } | Self::Line { .. } => None,
            Self::Face { body, .. } | Self::Body { body, .. } => Some(*body),
        }
    }

    /// The object for the kernel, with the shapes it needs.
    pub fn resolve<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
    ) -> Result<Target<K::Shape>, String> {
        self.resolve_linked(ctx, &BTreeMap::new())
    }

    /// [`Self::resolve`] where bodies of other components are read in their
    /// components and moved into the feature's coordinates by their
    /// `links` (mitcad#104, as a combine's tools).
    pub fn resolve_linked<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        links: &BTreeMap<BodyUid, OccurrenceLink>,
    ) -> Result<Target<K::Shape>, String> {
        self.resolve_across(ctx, links, None)
    }

    /// [`Self::resolve_linked`] where a point or a line stands for the
    /// plane through it of `across` (mitcad#1329: an extrusion's profile
    /// plane normal, a hole's axis). Without `across` both are refused.
    pub fn resolve_across<K: Kernel>(
        &self,
        ctx: &mut EvalContext<'_, K>,
        links: &BTreeMap<BodyUid, OccurrenceLink>,
        across: Option<Across<'_>>,
    ) -> Result<Target<K::Shape>, String> {
        let plane_normal = |what: &str| -> Result<[f64; 3], String> {
            across
                .and_then(|a| normalized(a.normal))
                .ok_or_else(|| format!("only extrusions and holes end at or start from {what}"))
        };
        // An extent up to a mesh body is refused (mitcad#399).
        let body_of = |ctx: &mut EvalContext<'_, K>, uid: BodyUid| match links.get(&uid) {
            None => ctx.surface_body(uid),
            Some(link) => {
                let (component, transform) =
                    ctx.linked(link).map_err(|e| format!("body {uid}: {e}"))?;
                let shape = ctx.body_in(component, uid)?;
                ctx.require_surfaces(uid, &shape)?;
                ctx.kernel
                    .transform_shape(&shape, &transform, None)
                    .map_err(|e| format!("body {uid}: {e}"))
            }
        };
        Ok(match self {
            Self::Plane { plane } => {
                let plane = ctx.plane(plane)?;
                // `+ 0.0` turns -0 into 0.
                Target::Plane(Plane {
                    origin: plane.origin.map(|v| v + 0.0),
                    normal: plane.normal().map(|v| v + 0.0),
                })
            }
            Self::Face {
                body,
                face,
                chained,
            } => {
                let mut shape = body_of(ctx, *body)?;
                let has = |ctx: &EvalContext<'_, K>, shape: &K::Shape| {
                    ctx.kernel
                        .count_faces(shape, face)
                        .map_err(|e| e.to_string())
                        .map(|n| n > 0)
                };
                let mut found = face.clone();
                if !has(ctx, &shape)? {
                    // A face a sketch change renamed (mitcad#272), else a
                    // pattern's copy: the face as the original found it,
                    // where the original itself took it away (a join up to
                    // the face of the body it joins).
                    match super::renamed::find_face(ctx.kernel, &shape, face) {
                        Ok(Some(renamed)) => {
                            ctx.found_renamed("face", face, &renamed, face.feature);
                            found = renamed;
                        }
                        Err(super::renamed::Lost::Ambiguous(message)) => return Err(message),
                        _ => {
                            let before = match ctx.objects_before {
                                Some(original) if !links.contains_key(body) => {
                                    ctx.body_before(original, *body)
                                }
                                _ => None,
                            };
                            match before {
                                Some(earlier) if has(ctx, &earlier)? => shape = earlier,
                                _ => {
                                    return Err(super::geom_ref::placed_at(
                                        crate::notice::NoticePlace::face(face.to_string()),
                                        *body,
                                        "the body has no such face",
                                        format!("body {body} has no face {face}"),
                                    ));
                                }
                            }
                        }
                    }
                }
                Target::Face {
                    body: shape,
                    face: found,
                    extend: !chained,
                }
            }
            Self::Body { body, through } => Target::Body {
                body: body_of(ctx, *body)?,
                through: *through,
            },
            Self::Point { point } => {
                let normal = plane_normal("a point")?;
                let origin = ctx.point(point)?;
                // `+ 0.0` turns -0 into 0.
                Target::Plane(Plane {
                    origin: origin.map(|v| v + 0.0),
                    normal: normal.map(|v| v + 0.0),
                })
            }
            Self::Line { line } => {
                let normal = plane_normal("a line")?;
                let lies = across.map_or("", |a| a.lies);
                // A straight edge only: a circular edge resolves to its
                // axis, which is no line of the design.
                if let GeomRef::Edge { edge, .. } = line
                    && !matches!(ctx.curve(line)?, CurveGeometry::Line { .. })
                {
                    return Err(format!(
                        "edge {edge} is not straight: a line object is a straight line"
                    ));
                }
                let axis = ctx.axis(line)?;
                let direction: [f64; 3] =
                    normalized(axis.direction).ok_or_else(|| format!("{line} has no direction"))?;
                // How far the line leans out of the planes of `normal`.
                let off = dot(direction, normal).abs().min(1.0).asin();
                if off > LINE_ANGLE {
                    return Err(format!(
                        "{line} does not lie {lies}: it is {:.3} degrees off",
                        off.to_degrees()
                    ));
                }
                Target::Plane(Plane {
                    origin: axis.origin.map(|v| v + 0.0),
                    normal: normal.map(|v| v + 0.0),
                })
            }
        })
    }
}

/// A face of a body.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceRef {
    pub body: BodyUid,
    pub face: FaceName,
}

impl FaceRef {
    pub fn add_references(&self, references: &mut References) {
        references.body(self.body);
        references.features.extend(self.face.features());
    }

    pub fn check(&self, ctx: &CheckContext<'_>) -> Result<(), String> {
        ctx.body(self.body)?;
        for feature in self.face.features() {
            ctx.feature(feature).map_err(|e| {
                crate::notice::about_place(
                    crate::notice::NoticePlace::face(self.face.to_string()),
                    &e.to_string(),
                )
            })?;
        }
        Ok(())
    }
}

// The unit vector of `v` (each component divided by the length); None for a
// zero vector.
pub(crate) use mitcad_vec3::unit_div as normalized;

/// The axis projected into the sketch plane (`isProjectAxis` in .f3d designs).
pub(crate) fn project_axis(axis: Axis, frame: &SketchFrame) -> Result<Axis, String> {
    let normal = frame.normal();
    let height = dot(
        std::array::from_fn(|i| axis.origin[i] - frame.origin[i]),
        normal,
    );
    let along = dot(axis.direction, normal);
    let direction = normalized(std::array::from_fn(|i| {
        axis.direction[i] - along * normal[i]
    }))
    .ok_or("the axis is perpendicular to the sketch plane and cannot be projected onto it")?;
    Ok(Axis {
        origin: std::array::from_fn(|i| axis.origin[i] - height * normal[i]),
        direction,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datum::OriginDatum;
    use crate::ids::{EntityUid, FeatureUid};

    #[test]
    fn objects_and_axes_serialize() {
        let face: ExtentObject = serde_json::from_value(serde_json::json!(
            {"type": "face", "body": "F2.b0", "face": "F2:end(r{c5})"}))
        .unwrap();
        assert_eq!(
            serde_json::to_value(&face).unwrap(),
            serde_json::json!({"type": "face", "body": "F2.b0", "face": "F2:end(r{c5})"})
        );
        let plane: ExtentObject =
            serde_json::from_value(serde_json::json!({"type": "plane", "plane": "xz"})).unwrap();
        assert_eq!(
            plane,
            ExtentObject::Plane {
                plane: GeomRef::Origin(OriginDatum::Xz)
            }
        );
        let construction: ExtentObject =
            serde_json::from_value(serde_json::json!({"type": "plane", "plane": "F5"})).unwrap();
        assert_eq!(
            construction,
            ExtentObject::Plane {
                plane: GeomRef::Datum(FeatureUid(5))
            }
        );
        // Revolution axes: sketch lines and other axis references.
        let line: GeomRef =
            serde_json::from_value(serde_json::json!({"sketch": "F1", "curve": "c4"})).unwrap();
        assert_eq!(
            line,
            GeomRef::SketchCurves {
                sketch: FeatureUid(1),
                curves: vec![EntityUid(4)]
            }
        );
        assert_eq!(
            serde_json::to_value(&line).unwrap(),
            serde_json::json!({"sketch": "F1", "curve": "c4"})
        );
        let origin: GeomRef = serde_json::from_value(serde_json::json!("y")).unwrap();
        assert_eq!(origin, GeomRef::Origin(OriginDatum::Y));
        assert!(serde_json::from_value::<GeomRef>(serde_json::json!("w")).is_err());
        let edge: GeomRef = serde_json::from_value(serde_json::json!({"body": "F2.b0",
                "edge": "E{F2:side(c1[c4,c2])|F2:side(c2[c1,c3])}"}))
        .unwrap();
        assert!(matches!(edge, GeomRef::Edge { .. }));
    }

    #[test]
    fn axes_project_into_the_sketch_plane() {
        let lifted = Axis {
            origin: [0.0, 0.0, 5.0],
            direction: normalized([0.0, 1.0, 1.0]).unwrap(),
        };
        let projected = project_axis(lifted, &SketchFrame::XY).unwrap();
        assert_eq!(projected.origin, [0.0, 0.0, 0.0]);
        assert!((projected.direction[1] - 1.0).abs() < 1e-12);
        let up = Axis {
            origin: [0.0; 3],
            direction: [0.0, 0.0, 1.0],
        };
        assert!(project_axis(up, &SketchFrame::XY).is_err());
    }
}
